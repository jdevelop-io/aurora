use aurora_core::scheduler::{BeamStatus, SchedulerEvent};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct BeamView {
    pub name: String,
    pub status: BeamStatus,
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    pub started_at: Option<Instant>,
}

impl BeamView {
    pub fn new(name: String) -> Self {
        BeamView {
            name,
            status: BeamStatus::Pending,
            stdout: vec![],
            stderr: vec![],
            started_at: None,
        }
    }

    pub fn status_symbol(&self) -> &str {
        match &self.status {
            BeamStatus::Pending => "─",
            BeamStatus::Running => "⣴",
            BeamStatus::Success { cached: true, .. } => "✦",
            BeamStatus::Success { cached: false, .. } => "✔",
            BeamStatus::Skipped { .. } => "◌",
            BeamStatus::Failed { .. } => "✕",
            BeamStatus::Cancelled => "✕",
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum AppMode {
    Running,
    LogView,
    Done { success: bool },
}

pub struct App {
    pub beams: Vec<BeamView>,
    pub mode: AppMode,
    pub selected: usize,
    pub log_scroll: u16,
}

impl App {
    pub fn new(beam_names: Vec<String>) -> Self {
        App {
            beams: beam_names.into_iter().map(BeamView::new).collect(),
            mode: AppMode::Running,
            selected: 0,
            log_scroll: 0,
        }
    }

    pub fn apply_event(&mut self, event: SchedulerEvent) {
        match event {
            SchedulerEvent::BeamStarted { name } => {
                if let Some(b) = self.beams.iter_mut().find(|b| b.name == name) {
                    b.status = BeamStatus::Running;
                    b.started_at = Some(Instant::now());
                }
            }
            SchedulerEvent::BeamCompleted { name, status } => {
                if let Some(b) = self.beams.iter_mut().find(|b| b.name == name) {
                    b.status = status;
                }
            }
            SchedulerEvent::BeamOutput { name, line, is_stderr } => {
                if let Some(b) = self.beams.iter_mut().find(|b| b.name == name) {
                    if is_stderr {
                        b.stderr.push(line);
                    } else {
                        b.stdout.push(line);
                    }
                }
            }
            SchedulerEvent::AllDone { success } => {
                self.mode = AppMode::Done { success };
            }
        }
    }

    pub fn select_next(&mut self) {
        self.selected = (self.selected + 1).min(self.beams.len().saturating_sub(1));
    }

    pub fn select_prev(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
}
