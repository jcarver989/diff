use crate::{
    WINDOW_TITLE,
    args::CliArgs,
    preferences::{ThemeStore, ViewModeStore},
};
use clankerdiff_client::DiffClient;
use clankerdiff_core::{DiffReviewCommand, DiffReviewEvent, DiffScope, Lookup, LookupReply, ReviewSubmission};
use clankerdiff_gpui::{
    DiffClientEvent, DiffClientView, DiffViewerOptions, RunCommand, ThemeChanged, ViewModeChanged,
    ui::{
        InterfaceSize,
        prelude::{EmptyState, NoticeTone, UI_FONT, UiTheme, binding_label, key_binding, window_appearance},
        window_chrome,
    },
};
use clankerdiff_gpui_shell::{run_loop, runtime};
use clankerdiff_server::{DiffServer, ServerOptions};
use gpui::{
    AnyElement, App, AppContext, Context, Entity, FocusHandle, KeyContext, Subscription, Task, Window, div, prelude::*,
};
use std::{path::PathBuf, sync::mpsc::Sender};

const KEY_CONTEXT: &str = "DesktopDiff";

pub(crate) struct DesktopApp {
    repository_path: PathBuf,
    scope: DiffScope,
    commits: bool,
    client: Option<DiffClient>,
    server: Option<DiffServer>,
    error: Option<String>,
    view: Entity<DiffClientView>,
    focus_handle: FocusHandle,
    load_task: Option<Task<()>>,
    command_task: Option<Task<()>>,
    watch_task: Task<()>,
    outcome_sender: Option<Sender<Option<ReviewSubmission>>>,
    _subscriptions: [Subscription; 4],
}

impl DesktopApp {
    pub(crate) fn new(
        args: CliArgs,
        client: Option<DiffClient>,
        outcome_sender: Option<Sender<Option<ReviewSubmission>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = ThemeStore::user().theme(window_appearance(window));
        let options =
            DiffViewerOptions { view_mode: ViewModeStore::user().view_mode(), ..DiffViewerOptions::default() };
        let view = cx.new(|_| DiffClientView::with_options(theme, options));
        let subscriptions = [
            cx.subscribe(&view, |this, _, event: &DiffClientEvent, cx| {
                this.on_client_event(event, cx);
            }),
            cx.subscribe_in(&view, window, |_, _, event: &ThemeChanged, window, _| {
                let _ = ThemeStore::user().remember(window_appearance(window), &event.id);
            }),
            cx.subscribe(&view, |_, _, event: &ViewModeChanged, _| {
                let _ = ViewModeStore::user().remember(event.mode);
            }),
            cx.observe_window_appearance(window, |this, window, cx| {
                let theme = ThemeStore::user().theme(window_appearance(window));
                this.view.update(cx, |view, cx| view.set_theme(theme, cx));
            }),
        ];
        let mut app = Self {
            repository_path: args.repository,
            scope: args.scope,
            commits: args.commits,
            client: None,
            server: None,
            error: None,
            view,
            focus_handle: cx.focus_handle(),
            load_task: None,
            command_task: None,
            watch_task: Task::ready(()),
            outcome_sender,
            _subscriptions: subscriptions,
        };
        if let Some(client) = client {
            app.install_client(client, cx);
        } else {
            app.discover(cx);
        }
        app
    }

    pub(crate) fn bind_keys(cx: &mut App) {
        let bindings = [
            key_binding("secondary-shift-s", RunCommand(DiffReviewCommand::StageAll), Some(KEY_CONTEXT), cx),
            key_binding("secondary-shift-u", RunCommand(DiffReviewCommand::UnstageAll), Some(KEY_CONTEXT), cx),
            key_binding("secondary-r", RunCommand(DiffReviewCommand::Refresh), Some(KEY_CONTEXT), cx),
        ];
        cx.bind_keys(bindings);
    }

    fn discover(&mut self, cx: &mut Context<Self>) {
        if self.load_task.is_some() {
            return;
        }
        let path = self.repository_path.clone();
        let (scope, commits) = (self.scope, self.commits);
        let operation = runtime::spawn(cx, async move {
            let server = DiffServer::open(path, ServerOptions::default()).await.map_err(|error| error.to_string())?;
            let transport = server.connect().map_err(|error| error.to_string())?;
            let target = server.initial_target(scope, commits);
            let client =
                DiffClient::from_transport(transport, target.into()).await.map_err(|error| error.to_string())?;
            Ok::<_, String>((server, client))
        });
        self.load_task = Some(cx.spawn(async move |this, cx| {
            let result = operation.await;
            let _ = this.update(cx, |this, cx| {
                this.load_task = None;
                match result {
                    Ok(Ok((server, client))) => {
                        this.server = Some(server);
                        this.install_client(client, cx);
                    }
                    Ok(Err(error)) => this.error = Some(error),
                    Err(error) => this.error = Some(format!("background task failed: {error}")),
                }
                cx.notify();
            });
        }));
    }

    fn install_client(&mut self, client: DiffClient, cx: &mut Context<Self>) {
        let mut updates = client.subscribe();
        let latest = updates.latest();
        self.view.update(cx, |view, cx| view.set_state(&latest, cx));
        self.client = Some(client);
        let view = self.view.downgrade();
        self.watch_task = cx.spawn(async move |_, cx| {
            while let Ok(state) = updates.changed().await {
                if view.update(cx, |view, cx| view.set_state(&state, cx)).is_err() {
                    return;
                }
            }
        });
    }

    fn on_client_event(&mut self, event: &DiffClientEvent, cx: &mut Context<Self>) {
        match (event, &self.outcome_sender) {
            (DiffClientEvent::Command(event), _) => self.command(event.clone(), cx),
            (DiffClientEvent::Submit(submission), Some(sender)) => {
                let _ = sender.send(Some(submission.clone()));
                run_loop::stop(cx);
            }
            (DiffClientEvent::Submit(submission), None) => println!(
                "{}",
                serde_json::to_string_pretty(submission)
                    .unwrap_or_else(|error| format!("{{\"serialization_error\":\"{error}\"}}"))
            ),
            (DiffClientEvent::Cancel, sender) => {
                if let Some(sender) = sender {
                    let _ = sender.send(None);
                }
                run_loop::stop(cx);
            }
            (DiffClientEvent::Lookup(lookup), _) => self.lookup(lookup.clone(), cx),
            (DiffClientEvent::ReviewChanged { .. }, _) => {}
        }
    }

    fn lookup(&mut self, lookup: Lookup, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let request = lookup.clone();
        let operation = runtime::spawn(cx, async move { client.lookup(request).await });
        cx.spawn(async move |this, cx| {
            let reply = operation
                .await
                .unwrap_or_else(|error| LookupReply::new(lookup, Err(format!("background task failed: {error}"))));
            let _ = this.update(cx, |this, cx| this.view.update(cx, |view, cx| view.lookup_finished(reply, cx)));
        })
        .detach();
    }

    fn command(&mut self, event: DiffReviewEvent, cx: &mut Context<Self>) {
        let Some(client) = self.client.clone() else {
            return;
        };
        let operation =
            runtime::spawn(cx, async move { client.handle(event).await.map_err(|error| error.to_string()) });
        self.command_task = Some(cx.spawn(async move |this, cx| {
            let result = match operation.await {
                Ok(result) => result,
                Err(error) => Err(format!("background task failed: {error}")),
            };
            let _ = this.update(cx, |this, cx| {
                this.command_task = None;
                this.view.update(cx, |view, cx| view.command_finished(result, cx));
            });
        }));
    }

    fn dispatch_command(&mut self, command: DiffReviewCommand, window: &mut Window, cx: &mut Context<Self>) {
        self.view.update(cx, |view, cx| view.handle_command(command, window, cx));
    }

    fn run_command(&mut self, action: &RunCommand, window: &mut Window, cx: &mut Context<Self>) {
        if action.0 == DiffReviewCommand::Refresh && self.error.is_some() {
            self.retry(window, cx);
        } else {
            self.dispatch_command(action.0.clone(), window, cx);
        }
    }

    fn retry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus_handle.is_focused(window) {
            window.blur(cx);
        }
        self.error = None;
        self.discover(cx);
        cx.notify();
    }

    fn error_detail(error: &str, window: &Window, cx: &App) -> String {
        let refresh = RunCommand(DiffReviewCommand::Refresh);
        KeyContext::parse(KEY_CONTEXT)
            .ok()
            .and_then(|context| window.highest_precedence_binding_for_action_in_context(&refresh, context))
            .map_or_else(
                || error.to_owned(),
                |binding| format!("{error} · press {} to retry", binding_label(&binding, cx)),
            )
    }

    fn status_panel(&self, title: String, detail: &str, tone: NoticeTone, cx: &App) -> AnyElement {
        let theme = UiTheme::new(self.view.read(cx).theme());
        EmptyState::new(title, detail.to_owned(), tone, theme).into_any_element()
    }
}

impl Render for DesktopApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        InterfaceSize::apply(window, cx);
        let theme = UiTheme::new(self.view.read(cx).theme());
        let body = if let Some(error) = &self.error {
            if window.focused(cx).is_none() {
                self.focus_handle.focus(window, cx);
            }
            let detail = Self::error_detail(error, window, cx);
            self.status_panel("Could not load diff".to_owned(), &detail, NoticeTone::Error, cx)
        } else if self.client.is_none() {
            self.status_panel("Loading diff…".to_owned(), "Git is reading the repository", NoticeTone::Info, cx)
        } else {
            self.view.clone().into_any_element()
        };
        let content = div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::run_command))
            .size_full()
            .bg(theme.colors.canvas)
            .font_family(UI_FONT)
            .child(body)
            .into_any_element();
        window_chrome::decorate(content, WINDOW_TITLE, &theme, window, cx)
    }
}
