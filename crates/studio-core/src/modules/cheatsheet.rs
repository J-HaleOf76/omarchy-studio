//! Keymap cheatsheet export (ROADMAP 0.9.5): the *effective* keymap — what
//! Hyprland is actually doing right now, attributed to the layer that
//! defines each bind — rendered as one self-contained HTML file themed from
//! the active palette. Group order and source tags mirror the Keybinds
//! screen; the point is a printable / shareable sheet, so the CSS carries a
//! print variant and the file needs no network or assets.

use crate::modules::dispatchers::{self, Category};
use crate::modules::keybinds::AttributedBind;
use crate::modules::themesync::Pal;

/// Fixed section order — same grouping the Keybinds screen uses.
const GROUPS: [Category; 9] = [
    Category::Session,
    Category::Apps,
    Category::Window,
    Category::Focus,
    Category::Workspace,
    Category::Group,
    Category::Layout,
    Category::Media,
    Category::Other,
];

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The layer tag shown next to a bind — where it's defined.
fn source_tag(b: &AttributedBind) -> &'static str {
    use crate::configfs::Layer::*;
    match b.source.as_ref().map(|s| s.layer) {
        Some(Default) => "omarchy",
        Some(Theme) => "theme",
        Some(User) => "user",
        Some(Toggle) => "toggle",
        Some(Runtime) | None => "runtime",
    }
}

/// What the bind does, human-first: its own description when the config line
/// carries one, else the dispatcher's label plus the argument.
fn action_text(b: &AttributedBind) -> String {
    if !b.runtime.description.is_empty() {
        return b.runtime.description.clone();
    }
    let label = dispatchers::label_for(&b.runtime.dispatcher);
    if b.runtime.arg.is_empty() {
        label.to_string()
    } else {
        format!("{label} · {}", b.runtime.arg)
    }
}

fn kbd_chord(chord: &str) -> String {
    chord
        .split('+')
        .map(|k| format!("<kbd>{}</kbd>", esc(k)))
        .collect::<Vec<_>>()
        .join("<span class=\"plus\">+</span>")
}

/// Render the sheet. `generated` is the footer credit (version, and a date
/// if the caller wants one) — injected so tests stay deterministic.
pub fn render(binds: &[AttributedBind], pal: &Pal, generated: &str) -> String {
    let mut sections = String::new();
    for cat in GROUPS {
        let rows: Vec<&AttributedBind> = binds
            .iter()
            .filter(|b| {
                dispatchers::lookup(&b.runtime.dispatcher)
                    .map(|d| d.category)
                    .unwrap_or(Category::Other)
                    == cat
            })
            .collect();
        if rows.is_empty() {
            continue;
        }
        sections.push_str(&format!(
            "<section>\n<h2>{} <span class=\"count\">{}</span></h2>\n<table>\n",
            esc(cat.label()),
            rows.len()
        ));
        for b in rows {
            let submap = if b.runtime.submap.is_empty() {
                String::new()
            } else {
                format!(" <span class=\"tag\">{}</span>", esc(&b.runtime.submap))
            };
            sections.push_str(&format!(
                "<tr><td class=\"chord\">{}</td><td>{}{submap}</td>\
                 <td class=\"src src-{}\">{}</td></tr>\n",
                kbd_chord(&b.runtime.chord()),
                esc(&action_text(b)),
                source_tag(b),
                source_tag(b),
            ));
        }
        sections.push_str("</table>\n</section>\n");
    }

    let Pal {
        bg,
        fg,
        accent,
        sel_bg,
        ..
    } = pal;
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Keymap cheatsheet</title>
<style>
:root {{ --bg: {bg}; --fg: {fg}; --accent: {accent}; --panel: {sel_bg}; }}
* {{ box-sizing: border-box; margin: 0; }}
body {{
  background: var(--bg); color: var(--fg);
  font: 14px/1.45 system-ui, sans-serif;
  padding: 2rem; max-width: 72rem; margin-inline: auto;
}}
h1 {{ color: var(--accent); font-size: 1.5rem; margin-bottom: .25rem; }}
p.sub {{ opacity: .6; margin-bottom: 1.5rem; }}
main {{ columns: 2 24rem; column-gap: 2rem; }}
section {{ break-inside: avoid; margin-bottom: 1.5rem; }}
h2 {{
  font-size: .95rem; text-transform: uppercase; letter-spacing: .06em;
  color: var(--accent); border-bottom: 1px solid var(--panel);
  padding-bottom: .3rem; margin-bottom: .4rem;
}}
h2 .count {{ opacity: .5; font-weight: normal; font-size: .8rem; }}
table {{ width: 100%; border-collapse: collapse; }}
td {{ padding: .22rem .4rem .22rem 0; vertical-align: top; }}
td.chord {{ white-space: nowrap; width: 1%; }}
kbd {{
  /* fg-tinted chip, not --panel: a theme's selection color can sit right on
     top of its foreground, which would make the key text invisible */
  background: color-mix(in srgb, var(--fg) 12%, var(--bg));
  border: 1px solid color-mix(in srgb, var(--fg) 25%, transparent);
  border-radius: 4px; padding: .05rem .4rem; font: .82rem ui-monospace, monospace;
}}
.plus {{ opacity: .45; padding: 0 .15rem; }}
.tag {{ font-size: .75rem; opacity: .6; border: 1px solid var(--panel); border-radius: 4px; padding: 0 .3rem; }}
td.src {{ text-align: right; font-size: .75rem; opacity: .55; width: 1%; white-space: nowrap; }}
footer {{ margin-top: 2rem; opacity: .5; font-size: .8rem; }}
@media print {{
  body {{ background: #fff; color: #000; padding: 0; }}
  :root {{ --panel: #eee; }}
  h1, h2 {{ color: #000; }}
  kbd {{ border-color: #999; }}
}}
</style>
</head>
<body>
<h1>Keymap cheatsheet</h1>
<p class="sub">the effective Hyprland keymap, tagged with where each bind is defined</p>
<main>
{sections}</main>
<footer>{generated}</footer>
</body>
</html>
"#,
        generated = esc(generated)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configfs::Layer;
    use crate::modules::keybinds::{BindSource, ConfigBind, RuntimeBind};

    fn bind(
        modmask: u16,
        key: &str,
        dispatcher: &str,
        arg: &str,
        desc: &str,
        layer: Option<Layer>,
    ) -> AttributedBind {
        AttributedBind {
            runtime: RuntimeBind {
                modmask,
                key: key.into(),
                keycode: 0,
                dispatcher: dispatcher.into(),
                arg: arg.into(),
                submap: String::new(),
                description: desc.into(),
                locked: false,
                release: false,
                repeat: false,
            },
            source: layer.map(|l| BindSource {
                layer: l,
                config: ConfigBind {
                    flags: "bind".into(),
                    modmask,
                    key: key.into(),
                    description: None,
                    dispatcher: dispatcher.into(),
                    arg: arg.into(),
                },
            }),
        }
    }

    #[test]
    fn renders_grouped_escaped_and_attributed() {
        let pal = Pal::from_palette(&crate::modules::themes::Palette::parse("").unwrap());
        let binds = vec![
            bind(
                64,
                "Q",
                "killactive",
                "",
                "Close window",
                Some(Layer::Default),
            ),
            bind(64, "T", "exec", "foo <bar>", "", Some(Layer::User)),
            bind(65, "F", "fullscreen", "1", "", None),
        ];
        let html = render(&binds, &pal, "omarchy-studio 0.0.0-test");

        assert!(html.starts_with("<!doctype html>"));
        // grouped under the dispatcher's category, chord split into keys
        assert!(html.contains("<h2>Window"));
        assert!(html.contains("<kbd>SUPER</kbd>"));
        assert!(html.contains("<kbd>Q</kbd>"));
        // description wins; exec falls back to label + arg, HTML-escaped
        assert!(html.contains("Close window"));
        assert!(html.contains("foo &lt;bar&gt;"));
        // source attribution tags
        assert!(html.contains(">omarchy</td>"));
        assert!(html.contains(">user</td>"));
        assert!(html.contains(">runtime</td>"));
        // themed from the palette + footer credit
        assert!(html.contains("--bg: #1a1a1a"));
        assert!(html.contains("omarchy-studio 0.0.0-test"));
        // no external references — self-contained
        assert!(!html.contains("http://") && !html.contains("https://"));
    }

    #[test]
    fn empty_categories_are_omitted() {
        let pal = Pal::from_palette(&crate::modules::themes::Palette::parse("").unwrap());
        let html = render(
            &[bind(64, "M", "exit", "", "", Some(Layer::Default))],
            &pal,
            "x",
        );
        assert!(html.contains("<h2>Session"));
        assert!(!html.contains("<h2>Media"));
    }
}
