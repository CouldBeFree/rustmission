use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;

use crate::{
    transmission::TorrentAction,
    tui::{
        app,
        components::{Component, ComponentAction, InputManager},
    },
};
use rm_shared::{
    action::{Action, UpdateAction},
    status_task::StatusTask,
};

pub struct AddMagnetBar {
    input_magnet_mgr: InputManager,
    input_location_mgr: InputManager,
    stage: Stage,
    ctx: app::Ctx,
}

enum Stage {
    AskMagnet,
    AskLocation,
}

impl AddMagnetBar {
    pub fn new(ctx: app::Ctx) -> Self {
        Self {
            input_magnet_mgr: InputManager::new("Add (Magnet URL / Torrent path): ".to_string()),
            input_location_mgr: InputManager::new_with_value(
                "Directory: ".to_string(),
                ctx.session_info.download_dir.clone(),
            ),
            stage: Stage::AskMagnet,
            ctx,
        }
    }

    fn handle_input(&mut self, input: KeyEvent) -> ComponentAction {
        match self.stage {
            Stage::AskMagnet => self.handle_magnet_input(input),
            Stage::AskLocation => self.handle_location_input(input),
        }
    }

    fn handle_magnet_input(&mut self, input: KeyEvent) -> ComponentAction {
        if input.code == KeyCode::Enter {
            self.stage = Stage::AskLocation;
            self.ctx.send_action(Action::Render);
            return ComponentAction::Nothing;
        }

        if input.code == KeyCode::Esc {
            return ComponentAction::Quit;
        }

        if self.input_magnet_mgr.handle_key(input).is_some() {
            self.ctx.send_action(Action::Render);
        }

        ComponentAction::Nothing
    }

    fn handle_location_input(&mut self, input: KeyEvent) -> ComponentAction {
        if input.code == KeyCode::Enter {
            let torrent_action = TorrentAction::Add(
                self.input_magnet_mgr.text(),
                Some(self.input_location_mgr.text()),
            );
            self.ctx.send_torrent_action(torrent_action);

            let task = StatusTask::new_add(self.input_magnet_mgr.text());
            self.ctx.send_update_action(UpdateAction::TaskSet(task));

            ComponentAction::Quit
        } else if input.code == KeyCode::Esc {
            ComponentAction::Quit
        } else if self.input_location_mgr.handle_key(input).is_some() {
            self.ctx.send_action(Action::Render);
            ComponentAction::Nothing
        } else {
            ComponentAction::Nothing
        }
    }
}

impl Component for AddMagnetBar {
    #[must_use]
    fn handle_actions(&mut self, action: Action) -> ComponentAction {
        match action {
            Action::Input(input) => self.handle_input(input),
            _ => ComponentAction::Nothing,
        }
    }

    fn render(&mut self, f: &mut Frame, rect: Rect) {
        match self.stage {
            Stage::AskMagnet => self.input_magnet_mgr.render(f, rect),
            Stage::AskLocation => self.input_location_mgr.render(f, rect),
        }
    }
}
