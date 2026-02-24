pub fn render_dashboard(tasks_count: usize, entities_count: usize) -> String {
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Borg Dashboard</title>
    <style>
      :root {{
        --bg: #0f1720;
        --panel: #172433;
        --ink: #e6f0ff;
        --muted: #8fa6c2;
        --accent: #40c4ff;
      }}
      body {{
        margin: 0;
        font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        background: radial-gradient(circle at 20% 0%, #1a2c43, var(--bg));
        color: var(--ink);
      }}
      .wrap {{ max-width: 960px; margin: 48px auto; padding: 0 20px; }}
      h1 {{ font-size: 36px; margin: 0 0 8px; }}
      p {{ color: var(--muted); margin: 0 0 28px; }}
      .grid {{ display: grid; gap: 16px; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); }}
      .card {{
        background: linear-gradient(180deg, rgba(255,255,255,.03), rgba(255,255,255,.01));
        border: 1px solid rgba(255,255,255,.08);
        border-radius: 12px;
        padding: 18px;
      }}
      .label {{ color: var(--muted); font-size: 12px; text-transform: uppercase; letter-spacing: .08em; }}
      .value {{ font-size: 30px; color: var(--accent); margin-top: 8px; }}
      code {{ color: var(--accent); }}
    </style>
  </head>
  <body>
    <main class="wrap">
      <h1>Borg UI</h1>
      <p>Prototype control plane. For details use <code>/tasks</code> and <code>/memory/search?q=...</code>.</p>
      <section class="grid">
        <article class="card"><div class="label">Tasks</div><div class="value">{}</div></article>
        <article class="card"><div class="label">Knowledge Entities</div><div class="value">{}</div></article>
      </section>
    </main>
  </body>
</html>"#,
        tasks_count, entities_count
    )
}
