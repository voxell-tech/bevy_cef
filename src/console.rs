//! Pages' console messages for webviews.
//!
//! CEF's `DisplayHandler::on_console_message` sends each message a page writes to its console,
//! and each error that goes uncaught in it, across a channel. [`drain_console`] receives them on
//! the Bevy side and fires a [`ConsoleMessage`] entity event for each.

use bevy::prelude::*;
use bevy_cef_core::prelude::{ConsoleLevel, console_messages};

pub(super) struct ConsolePlugin;

impl Plugin for ConsolePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreUpdate, drain_console);
    }
}

/// Fired when a webview's page writes to its console, or an error goes uncaught in it.
#[derive(Debug, EntityEvent, Clone)]
pub struct ConsoleMessage {
    #[event_target]
    pub webview: Entity,
    pub level: ConsoleLevel,
    pub message: String,
    /// The script the message came from.
    pub source: String,
    pub line: i32,
}

fn drain_console(mut commands: Commands, webviews: Query<(), With<crate::prelude::WebviewSource>>) {
    while let Ok(message) = console_messages().1.try_recv() {
        if !webviews.contains(message.webview) {
            continue;
        }
        commands.trigger(ConsoleMessage {
            webview: message.webview,
            level: message.level,
            message: message.message,
            source: message.source,
            line: message.line,
        });
    }
}
