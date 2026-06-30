//! Rendu texte du mode headless : draine le flux d'événements du scheduler,
//! l'affiche en lignes préfixées par beam (stdout/stderr séparés), puis imprime
//! un récap final. Renvoie le succès global, qui pilote le code de sortie.

use std::io::Write;

use aurora_core::scheduler::{BeamStatus, SchedulerEvent, SkipReason};
use tokio::sync::mpsc;

/// Enrobe `text` dans un code couleur ANSI lorsque `use_color` est vrai.
fn paint(text: &str, code: &str, use_color: bool) -> String {
    if use_color {
        format!("\u{1b}[{code}m{text}\u{1b}[0m")
    } else {
        text.to_string()
    }
}

/// Formate une durée en secondes avec une décimale (ex. "4.2s").
fn fmt_duration(d: std::time::Duration) -> String {
    format!("{:.1}s", d.as_secs_f64())
}

/// Construit la ligne de récap d'un beam terminé.
/// Renvoie `None` pour les statuts non terminaux (Pending/Running), jamais émis ici.
fn recap_line(name: &str, status: &BeamStatus, width: usize, use_color: bool) -> Option<String> {
    let (marker, color, detail) = match status {
        BeamStatus::Success {
            duration,
            cached: false,
        } => ("OK", "32", fmt_duration(*duration)),
        BeamStatus::Success { cached: true, .. } => ("OK", "32", "cached".to_string()),
        BeamStatus::Skipped { reason } => {
            let r = match reason {
                SkipReason::Cached => "cached",
                SkipReason::ConditionFalse => "condition false",
            };
            ("SKIP", "33", r.to_string())
        }
        BeamStatus::Failed {
            exit_code,
            duration,
        } => (
            "FAIL",
            "31",
            format!("exit {exit_code} {}", fmt_duration(*duration)),
        ),
        BeamStatus::FailedAllowed {
            exit_code,
            duration,
        } => (
            "WARN",
            "33",
            format!("exit {exit_code} (allowed) {}", fmt_duration(*duration)),
        ),
        BeamStatus::Cancelled => ("CANC", "35", "cancelled".to_string()),
        BeamStatus::Pending | BeamStatus::Running => return None,
    };
    // Marqueur cadré sur 6 caractères ("[FAIL]") avant coloration, pour aligner
    // sans que les codes ANSI (largeur nulle) ne décalent les colonnes.
    let marker = format!("{:<6}", format!("[{marker}]"));
    let marker = paint(&marker, color, use_color);
    Some(format!("{marker} {name:<width$}  {detail}"))
}

/// Draine le flux d'événements jusqu'à `AllDone`, imprime les lignes préfixées
/// puis le récap. Renvoie le succès global porté par `AllDone`.
pub async fn run_headless(
    beam_names: &[String],
    use_color: bool,
    mut rx: mpsc::Receiver<SchedulerEvent>,
    out: &mut impl Write,
    err: &mut impl Write,
) -> std::io::Result<bool> {
    let width = beam_names.iter().map(|n| n.len()).max().unwrap_or(0);
    let mut recap: Vec<(String, BeamStatus)> = Vec::new();
    let mut overall = true;

    while let Some(event) = rx.recv().await {
        match event {
            SchedulerEvent::BeamOutput {
                name,
                line,
                is_stderr,
            } => {
                let prefix = paint(&format!("[{name:<width$}]"), "90", use_color);
                if is_stderr {
                    writeln!(err, "{prefix} {line}")?;
                } else {
                    writeln!(out, "{prefix} {line}")?;
                }
            }
            SchedulerEvent::BeamCompleted { name, status } => recap.push((name, status)),
            SchedulerEvent::BeamStarted { .. } => {}
            SchedulerEvent::AllDone { success } => {
                overall = success;
                break;
            }
        }
    }

    writeln!(out)?;
    let mut ok = 0usize;
    let mut failed = 0usize;
    for (name, status) in &recap {
        if let Some(line) = recap_line(name, status, width, use_color) {
            writeln!(out, "{line}")?;
        }
        match status {
            BeamStatus::Success { .. }
            | BeamStatus::Skipped { .. }
            | BeamStatus::FailedAllowed { .. } => ok += 1,
            BeamStatus::Failed { .. } | BeamStatus::Cancelled => failed += 1,
            BeamStatus::Pending | BeamStatus::Running => {}
        }
    }
    writeln!(out, "Done: {ok} ok, {failed} failed")?;

    Ok(overall)
}
