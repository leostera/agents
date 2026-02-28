use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Result;
use borg_core::Entity;
use tantivy::{
    Index, IndexWriter, TantivyDocument,
    collector::TopDocs,
    query::QueryParser,
    schema::{Field, STORED, STRING, Schema, TEXT, Value as TantivyValue},
};
use tracing::info;

use crate::SearchQuery;

const FIELD_ENTITY_ID: &str = "entity_id";
const FIELD_TEXT: &str = "text";
const FIELD_NAMESPACE: &str = "namespace";
const FIELD_KIND: &str = "kind";
const FIELD_LABEL: &str = "label";

#[derive(Clone)]
pub(crate) struct TantivySearchIndex {
    writer: Arc<Mutex<IndexWriter>>,
    reader: Arc<tantivy::IndexReader>,
    fields: SearchFields,
    path: PathBuf,
}

#[derive(Clone)]
struct SearchFields {
    entity_id: Field,
    text: Field,
    namespace: Field,
    kind: Field,
    label: Field,
}

impl TantivySearchIndex {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        std::fs::create_dir_all(&path)?;
        let (index, fields) = open_or_create_index(&path)?;
        let writer = index.writer(50_000_000)?;
        let reader = index.reader()?;

        Ok(Self {
            writer: Arc::new(Mutex::new(writer)),
            reader: Arc::new(reader),
            fields,
            path,
        })
    }

    pub(crate) async fn migrate(&self) -> Result<()> {
        info!(
            target: "borg_memory",
            path = %self.path.display(),
            "tantivy search index ready"
        );
        Ok(())
    }

    pub(crate) async fn upsert_entity(&self, entity: &Entity) -> Result<()> {
        let namespace = entity
            .props
            .get("namespace")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let kind = entity
            .props
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or(entity.entity_type.as_str());
        let text = format!("{} {}", entity.label, entity.props);

        let mut doc = TantivyDocument::default();
        doc.add_text(self.fields.entity_id, entity.entity_id.as_str());
        doc.add_text(self.fields.namespace, namespace);
        doc.add_text(self.fields.kind, kind);
        doc.add_text(self.fields.label, &entity.label);
        doc.add_text(self.fields.text, &text);

        let mut writer = self
            .writer
            .lock()
            .map_err(|_| anyhow::anyhow!("search index writer poisoned"))?;
        writer.delete_term(tantivy::Term::from_field_text(
            self.fields.entity_id,
            entity.entity_id.as_str(),
        ));
        writer.add_document(doc)?;
        writer.commit()?;
        Ok(())
    }

    pub(crate) async fn search(&self, query: &SearchQuery, limit: usize) -> Result<Vec<String>> {
        let Some(query_text) = query.text() else {
            return Ok(Vec::new());
        };

        self.reader.reload()?;
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(
            searcher.index(),
            vec![
                self.fields.text,
                self.fields.label,
                self.fields.namespace,
                self.fields.kind,
            ],
        );
        let parsed = parser.parse_query(query_text)?;
        let top_docs = searcher.search(&parsed, &TopDocs::with_limit(limit))?;

        let mut out = Vec::new();
        for (_, addr) in top_docs {
            let doc = searcher.doc::<TantivyDocument>(addr)?;
            let ns_ok = if let Some(expected_ns) = &query.ns {
                doc.get_first(self.fields.namespace)
                    .and_then(|v| v.as_value().as_str())
                    == Some(expected_ns.as_str())
            } else {
                true
            };
            let kind_ok = if let Some(expected_kind) = &query.kind {
                doc.get_first(self.fields.kind)
                    .and_then(|v| v.as_value().as_str())
                    == Some(expected_kind.as_str())
            } else {
                true
            };
            if !(ns_ok && kind_ok) {
                continue;
            }

            if let Some(entity_id) = doc
                .get_first(self.fields.entity_id)
                .and_then(|v| v.as_value().as_str())
            {
                out.push(entity_id.to_string());
            }
        }
        Ok(out)
    }
}

fn open_or_create_index(path: &Path) -> Result<(Index, SearchFields)> {
    let schema = build_schema();
    let index = match Index::open_in_dir(path) {
        Ok(index) => index,
        Err(_) => Index::create_in_dir(path, schema.clone())?,
    };
    let fields = SearchFields {
        entity_id: schema.get_field(FIELD_ENTITY_ID)?,
        text: schema.get_field(FIELD_TEXT)?,
        namespace: schema.get_field(FIELD_NAMESPACE)?,
        kind: schema.get_field(FIELD_KIND)?,
        label: schema.get_field(FIELD_LABEL)?,
    };
    Ok((index, fields))
}

fn build_schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field(FIELD_ENTITY_ID, STRING | STORED);
    builder.add_text_field(FIELD_TEXT, TEXT | STORED);
    builder.add_text_field(FIELD_NAMESPACE, STRING | STORED);
    builder.add_text_field(FIELD_KIND, STRING | STORED);
    builder.add_text_field(FIELD_LABEL, TEXT | STORED);
    builder.build()
}
