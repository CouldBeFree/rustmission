pub mod popups;
mod stats;
pub mod task_manager;
pub mod tasks;

use std::sync::{Arc, Mutex};

use crate::ui::tabs::torrents::popups::stats::StatisticsPopup;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::prelude::*;
use ratatui::widgets::{Row, Table};
use ratatui_macros::constraints;
use transmission_rpc::types::{Torrent, TorrentStatus};

use crate::action::{Action, TorrentAction};
use crate::transmission::RustmissionTorrent;
use crate::ui::components::table::GenericTable;
use crate::ui::components::Component;
use crate::{app, transmission};

use self::stats::StatsComponent;
use self::task_manager::TaskManager;

pub struct TorrentsTab {
    table_manager: Arc<Mutex<TableManager>>,
    stats: StatsComponent,
    task: TaskManager,
    statistics_popup: Option<StatisticsPopup>,
    ctx: app::Ctx,
    header: Vec<String>,
}

pub struct TableManager {
    ctx: app::Ctx,
    table: Arc<Mutex<GenericTable<Torrent>>>,
    rows: Vec<RustmissionTorrent>,
    widths: [Constraint; 6],
    filter: Arc<Mutex<Option<String>>>,
}

impl TableManager {
    fn new(
        ctx: app::Ctx,
        table: Arc<Mutex<GenericTable<Torrent>>>,
        rows: Vec<RustmissionTorrent>,
    ) -> Self {
        let widths = Self::default_widths();
        TableManager {
            ctx,
            rows,
            table,
            widths,
            filter: Arc::new(Mutex::new(None)),
        }
    }

    fn default_widths() -> [Constraint; 6] {
        [
            Constraint::Max(70),    // Name
            Constraint::Length(10), // Size
            Constraint::Length(10), // Progress
            Constraint::Length(10), // ETA
            Constraint::Length(10), // Download
            Constraint::Length(10), // Upload
        ]
    }

    pub fn get_current_item(&self) -> Option<RustmissionTorrent> {
        let matcher = SkimMatcherV2::default();
        let index = {
            if let Some(index) = self.table.lock().unwrap().state.borrow().selected() {
                index
            } else {
                return None;
            }
        };

        if let Some(filter) = &*self.filter.lock().unwrap() {
            let filtered_rows: Vec<_> = self
                .rows
                .iter()
                .filter(|row| matcher.fuzzy_match(&row.torrent_name, &filter).is_some())
                .collect();
            return filtered_rows.get(index).cloned().cloned();
        }
        self.rows.get(index).cloned()
    }

    pub fn set_new_rows(&mut self, rows: Vec<RustmissionTorrent>) {
        let matcher = SkimMatcherV2::default();
        if let Some(filter) = &*self.filter.lock().unwrap() {
            self.rows = rows
                .into_iter()
                .filter(|row| matcher.fuzzy_match(&row.torrent_name, &filter).is_some())
                .collect();
        } else {
            self.rows = rows;
        };
        self.widths = self.header_widths(&self.rows);
    }

    fn header_widths(&self, rows: &[RustmissionTorrent]) -> [Constraint; 6] {
        if !self.ctx.config.general.auto_hide {
            return Self::default_widths();
        }

        let mut download_width = 0;
        let mut upload_width = 0;
        let mut progress_width = 0;
        let mut eta_width = 0;

        for row in rows {
            if !row.download_speed.is_empty() {
                download_width = 9;
            }
            if !row.upload_speed.is_empty() {
                upload_width = 9;
            }
            if !row.progress.is_empty() {
                progress_width = 9;
            }

            if !row.eta_secs.is_empty() {
                eta_width = 9;
            }
        }

        [
            Constraint::Max(70),                // Name
            Constraint::Length(9),              // Size
            Constraint::Length(progress_width), // Progress
            Constraint::Length(eta_width),      // ETA
            Constraint::Length(download_width), // Download
            Constraint::Length(upload_width),   // Upload
        ]
    }
}

impl TorrentsTab {
    pub fn new(ctx: app::Ctx) -> Self {
        let stats = StatsComponent::default();
        let table = Arc::new(Mutex::new(GenericTable::new(vec![])));
        let rows = vec![];

        let table_manager = Arc::new(Mutex::new(TableManager::new(
            ctx.clone(),
            Arc::clone(&table),
            rows,
        )));

        tokio::spawn(transmission::stats_fetch(
            ctx.clone(),
            Arc::clone(&stats.stats),
        ));

        tokio::spawn(transmission::torrent_fetch(
            ctx.clone(),
            Arc::clone(&table.lock().unwrap().items),
            Arc::clone(&table_manager),
        ));

        Self {
            stats,
            task: TaskManager::new(table_manager.clone(), ctx.clone()),
            table_manager,
            statistics_popup: None,
            ctx,
            header: vec![
                "Name".to_owned(),
                "Size".to_owned(),
                "Progress".to_owned(),
                "ETA".to_owned(),
                "Download".to_owned(),
                "Upload".to_owned(),
            ],
        }
    }

    fn header(&self) -> &Vec<String> {
        &self.header
    }
}

impl Component for TorrentsTab {
    fn render(&mut self, f: &mut Frame, rect: Rect) {
        let [torrents_list_rect, stats_rect] =
            Layout::vertical(constraints![>=10, ==1]).areas(rect);

        let table_manager = &self.table_manager.lock().unwrap();

        let rows = &table_manager.rows;

        let torrent_rows: Vec<_> = rows
            .iter()
            .map(|torrent| {
                crate::transmission::RustmissionTorrent::to_row(
                    torrent,
                    &table_manager.filter.lock().unwrap(),
                )
            })
            .filter_map(|row| row)
            .collect();

        table_manager
            .table
            .lock()
            .unwrap()
            .overwrite_len(torrent_rows.len());

        let highlight_table_style = Style::default().on_black().bold().fg(self
            .ctx
            .config
            .general
            .accent_color
            .as_ratatui());
        let table = Table::new(torrent_rows, table_manager.widths)
            .header(Row::new(self.header().iter().map(|s| s.as_str())))
            .highlight_style(highlight_table_style);

        f.render_stateful_widget(
            table,
            torrents_list_rect,
            &mut table_manager.table.lock().unwrap().state.borrow_mut(),
        );

        self.stats.render(f, stats_rect);

        self.task.render(f, stats_rect);

        if let Some(popup) = &mut self.statistics_popup {
            popup.render(f, f.size());
        }
    }

    #[must_use]
    fn handle_actions(&mut self, action: Action) -> Option<Action> {
        use Action as A;
        if let Some(popup) = &mut self.statistics_popup {
            if let Some(Action::Quit) = popup.handle_actions(action) {
                self.statistics_popup = None;
                return Some(Action::Render);
            };
            return None;
        }

        match action {
            A::Up => {
                self.table_manager
                    .lock()
                    .unwrap()
                    .table
                    .lock()
                    .unwrap()
                    .previous();
                Some(Action::Render)
            }
            A::Down => {
                self.table_manager
                    .lock()
                    .unwrap()
                    .table
                    .lock()
                    .unwrap()
                    .next();
                Some(Action::Render)
            }
            A::ShowStats => {
                if let Some(stats) = &*self.stats.stats.lock().unwrap() {
                    self.statistics_popup =
                        Some(StatisticsPopup::new(self.ctx.clone(), stats.clone()));
                    Some(Action::Render)
                } else {
                    None
                }
            }
            A::Pause => {
                let table_manager = self.table_manager.lock().unwrap();
                if let Some(torrent) = table_manager.get_current_item() {
                    let torrent_id = torrent.id.clone();
                    let torrent_status = torrent.status;
                    match torrent_status {
                        TorrentStatus::Stopped => {
                            self.ctx
                                .send_torrent_action(TorrentAction::Start(Box::new(vec![
                                    torrent_id,
                                ])));
                        }
                        _ => {
                            self.ctx
                                .send_torrent_action(TorrentAction::Stop(Box::new(vec![
                                    torrent_id,
                                ])));
                        }
                    }
                }
                None
            }

            other => self.task.handle_actions(other),
        }
    }
}