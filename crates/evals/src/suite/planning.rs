use std::collections::BTreeSet;

use super::*;

impl<'a, State, A> SuiteRunner<'a, State, A>
where
    State: Send + Sync + 'static,
    A: Agent,
{
    pub(crate) fn plan(self) -> EvalResult<SuitePlan<State, A>> {
        let mut suite = self.suite.clone();
        let mut config = self.config;
        let had_targets_before_filter = !config.targets.is_empty();
        config
            .targets
            .retain(|target| self.filter.matches_target(target));

        if config.targets.is_empty() {
            return if had_targets_before_filter {
                if let Some(model) = self.filter.model {
                    Err(EvalError::no_targets_matched_model(model))
                } else {
                    Err(EvalError::NoTargetsConfigured)
                }
            } else {
                Err(EvalError::NoTargetsConfigured)
            };
        }

        if let Some(query) = self.filter.query.as_deref()
            && !suite.id().contains(query)
        {
            let mut matched_eval_ids = BTreeSet::new();
            config.targets.retain(|target| {
                let matching_eval_ids = suite
                    .evals()
                    .iter()
                    .filter_map(|eval| {
                        let search_key = format!("{}::{}::{}", suite.id(), target.label, eval.id());
                        search_key.contains(query).then(|| eval.id().to_string())
                    })
                    .collect::<Vec<_>>();
                let target_has_match = !matching_eval_ids.is_empty();
                matched_eval_ids.extend(matching_eval_ids);
                target_has_match
            });
            suite
                .evals
                .retain(|eval| matched_eval_ids.contains(eval.id()));
        }

        if suite.evals.is_empty() || config.targets.is_empty() {
            return if let Some(query) = self.filter.query {
                Err(EvalError::no_matches_for_query(query))
            } else {
                Err(EvalError::suite_has_no_evals(suite.id()))
            };
        }

        Ok(SuitePlan {
            suite,
            config,
            artifact_root: self.artifact_root,
        })
    }
}
