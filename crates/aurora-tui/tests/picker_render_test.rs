use aurora_tui::app::{PickerBeam, PickerState};
use aurora_tui::picker::view::render_picker;
use ratatui::{backend::TestBackend, style::Color, Terminal};

fn beam(name: &str, description: Option<&str>, depends_on: Vec<&str>) -> PickerBeam {
    PickerBeam {
        name: name.to_string(),
        description: description.map(str::to_string),
        depends_on: depends_on.into_iter().map(str::to_string).collect(),
        signature: name.to_string(),
        requires_args: false,
    }
}

fn state() -> PickerState {
    PickerState::new(vec![
        beam("build", Some("compile"), vec!["fmt"]),
        beam("test", None, vec![]),
        beam("fmt", None, vec![]),
    ])
}

#[test]
fn render_picker_normal_size() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let st = state();
    terminal.draw(|f| render_picker(f, &st)).unwrap();
}

#[test]
fn render_picker_small_size() {
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    let st = state();
    terminal.draw(|f| render_picker(f, &st)).unwrap();
}

#[test]
fn render_picker_deps_open() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut st = state();
    st.show_deps = true;
    terminal.draw(|f| render_picker(f, &st)).unwrap();
}

/// A row with a param signature and a pending notice must render without
/// panicking: covers the dimmed/signature row and the footer notice line
/// added for parameterized beams.
#[test]
fn render_picker_with_parameterized_beam_and_notice() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut st = PickerState::new(vec![
        beam("fmt", None, vec![]),
        PickerBeam {
            name: "deploy".to_string(),
            description: None,
            depends_on: vec![],
            signature: "deploy <version> [env=staging]".to_string(),
            requires_args: true,
        },
    ]);
    st.selected = 1;
    st.notice = Some(
        "'deploy' requires arguments: run `aurora deploy <version> [env=staging]`".to_string(),
    );
    terminal.draw(|f| render_picker(f, &st)).unwrap();
}

/// Regression: a `requires_args` beam must stay dimmed even while the fuzzy
/// filter is active. The search branch runs `highlight_name`, which used to
/// have no dimmed case and rendered the matched name in Gray/White + Yellow,
/// leaving the row's checkbox/prefix DarkGray but its name undimmed. Asserts
/// on the produced buffer: every cell of the matched "deploy" name is
/// DarkGray, so the whole row reads as uniformly dimmed.
#[test]
fn parameterized_beam_stays_dimmed_under_the_filter() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut st = PickerState::new(vec![
        beam("fmt", None, vec![]),
        PickerBeam {
            name: "deploy".to_string(),
            description: None,
            depends_on: vec![],
            signature: "deploy <version> [env=staging]".to_string(),
            requires_args: true,
        },
    ]);
    // A non-empty query matching the param beam drives the search branch.
    st.search = "dep".to_string();
    terminal.draw(|f| render_picker(f, &st)).unwrap();

    let buffer = terminal.backend().buffer();
    let name: Vec<char> = "deploy".chars().collect();
    let area = *buffer.area();
    // Locate the contiguous run of cells spelling "deploy" (the beam name span)
    // and assert each of its cells is DarkGray.
    let mut found = false;
    'rows: for y in 0..area.height {
        for x in 0..area.width.saturating_sub(name.len() as u16) {
            let matches_here = name.iter().enumerate().all(|(k, ch)| {
                buffer
                    .cell((x + k as u16, y))
                    .map(|c| c.symbol() == ch.to_string())
                    .unwrap_or(false)
            });
            if matches_here {
                for k in 0..name.len() as u16 {
                    let cell = buffer.cell((x + k, y)).unwrap();
                    assert_eq!(
                        cell.fg,
                        Color::DarkGray,
                        "filtered param-beam name cell '{}' should be DarkGray",
                        cell.symbol()
                    );
                }
                found = true;
                break 'rows;
            }
        }
    }
    assert!(
        found,
        "the 'deploy' row must be present in the rendered buffer"
    );
}
