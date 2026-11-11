use crate::{args::CliArgs, preferences::Preferences};
use clankerdiff_client::DiffClient;
use clankerdiff_core::{DiffReviewCommand, DiffReviewEvent, Lookup, LookupReply, ReviewSubmission};
use clankerdiff_rouge::{DiffClientView, DiffViewerOptions, ReviewOutput, RunCommand};
use clankerdiff_rouge_shell::{
    rouge::{
        core::{Chord, Div, Key, Modifiers, Styled, View, WindowHandle, appearance, div, window_handle},
        reactive::{Stored, Task, effect, spawn, stored},
    },
    runtime,
};
use clankerdiff_server::{DiffServer, ServerOptions};
use std::sync::mpsc::Sender;

pub(crate) fn root(
    args: CliArgs,
    client: Option<DiffClient>,
    outcome: Option<Sender<Option<ReviewSubmission>>>,
    preferences: Preferences,
) -> impl View {
    let size = preferences.interface_size();
    let host = Host {
        view: stored(None),
        client: stored(None),
        outcome: stored(outcome),
        lookup: stored(None),
        window: window_handle(),
        preferences: stored(preferences.clone()),
    };
    let view = DiffClientView::mount(
        preferences.theme(),
        DiffViewerOptions { view_mode: preferences.view_mode(), ..DiffViewerOptions::default() },
        move |output| host.output(output),
    );
    host.view.set(Some(view));
    let appearance = appearance();
    effect(move || {
        appearance.get();
        view.review().set_theme(preferences.theme());
    });
    spawn(host.connect(args, client));
    let root = div()
        .size_full()
        .min_h(0.0)
        .flex()
        .col()
        .on_action(move |command: &RunCommand| {
            let _ = view.handle_command(command.0.clone());
        })
        .child(view.view());
    size.bind(bind_shortcuts(root, host.window))
}

#[derive(Clone, Copy)]
struct Host {
    view: Stored<Option<DiffClientView>>,
    client: Stored<Option<DiffClient>>,
    outcome: Stored<Option<Sender<Option<ReviewSubmission>>>>,
    lookup: Stored<Option<Task>>,
    window: WindowHandle,
    preferences: Stored<Preferences>,
}

impl Host {
    async fn connect(self, args: CliArgs, client: Option<DiffClient>) {
        let Some(view) = self.view.get() else {
            return;
        };
        let (server, client) = match open_client(args, client).await {
            Ok(pair) => pair,
            Err(error) => return view.set_failure(error),
        };
        let mut updates = client.subscribe();
        view.set_state(&updates.latest());
        self.client.set(Some(client));
        while let Ok(state) = updates.changed().await {
            view.set_state(&state);
        }
        drop(server);
    }

    fn output(self, output: ReviewOutput) {
        match output {
            ReviewOutput::ThemeChanged(id) => self.preferences.with(|preferences| preferences.remember_theme(&id)),
            ReviewOutput::ViewModeChanged(mode) => self.preferences.with(|preferences| preferences.remember_mode(mode)),
            ReviewOutput::Event(DiffReviewEvent::SubmitReview(submission)) => self.submit(submission),
            ReviewOutput::Event(DiffReviewEvent::Cancel) => self.window.quit(),
            ReviewOutput::Event(DiffReviewEvent::CopyFormattedReview(text)) => self.window.copy_text(text),
            ReviewOutput::Event(DiffReviewEvent::Lookup(request)) => self.lookup(request),
            ReviewOutput::Event(event) => self.repository(event),
        }
    }

    fn submit(self, submission: ReviewSubmission) {
        if let Some(outcome) = self.outcome.update(Option::take) {
            let _ = outcome.send(Some(submission));
            self.window.quit();
        } else if let Ok(json) = serde_json::to_string_pretty(&submission) {
            println!("{json}");
        }
    }

    fn lookup(self, request: Lookup) {
        let (Some(client), Some(view)) = (self.client.get(), self.view.get()) else {
            return;
        };
        let task = spawn(async move {
            let fallback = request.clone();
            let reply = runtime::spawn(async move { client.lookup(request).await })
                .await
                .unwrap_or_else(|error| LookupReply::new(fallback, Err(error.to_string())));
            view.lookup_finished(reply);
        });
        if let Some(previous) = self.lookup.update(|slot| slot.replace(task)) {
            previous.cancel();
        }
    }

    fn repository(self, event: DiffReviewEvent) {
        let (Some(client), Some(view)) = (self.client.get(), self.view.get()) else {
            return;
        };
        spawn(async move {
            let result = runtime::spawn(async move { client.handle(event).await.map_err(|error| error.to_string()) })
                .await
                .unwrap_or_else(|error| Err(error.to_string()));
            view.command_finished(result);
        });
    }
}

fn bind_shortcuts(root: Div, window: WindowHandle) -> Div {
    [
        ("secondary-shift-s", DiffReviewCommand::StageAll),
        ("secondary-shift-u", DiffReviewCommand::UnstageAll),
        ("secondary-r", DiffReviewCommand::Refresh),
    ]
    .into_iter()
    .fold(root, |root, (chord, command)| match chord.parse::<Chord>() {
        Ok(chord) => root.bind_key(chord, RunCommand(command)),
        Err(_) => root,
    })
    .bind_key(Chord::new(Key::character("q"), Modifiers::COMMAND), Quit)
    .on_action(move |_: &Quit| window.quit())
}

#[derive(Clone, Copy)]
struct Quit;

async fn open_client(args: CliArgs, client: Option<DiffClient>) -> Result<(Option<DiffServer>, DiffClient), String> {
    if let Some(client) = client {
        return Ok((None, client));
    }
    runtime::spawn(async move {
        let server =
            DiffServer::open(args.repository, ServerOptions::default()).await.map_err(|error| error.to_string())?;
        let transport = server.connect().map_err(|error| error.to_string())?;
        let target = server.initial_target(args.scope, args.commits);
        let client = DiffClient::from_transport(transport, target.into()).await.map_err(|error| error.to_string())?;
        Ok((Some(server), client))
    })
    .await
    .map_err(|error| error.to_string())?
}
