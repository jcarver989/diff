use crate::{args::CliArgs, preferences::Preferences};
use clankerdiff_client::DiffClient;
use clankerdiff_core::{DiffReviewCommand, DiffReviewEvent, LookupReply, ReviewSubmission};
use clankerdiff_rouge::{DiffClientView, DiffViewerOptions, ReviewOutput, RunCommand};
use clankerdiff_rouge_shell::{
    rouge::{
        core::{Chord, Div, Key, Modifiers, Styled, View, div, window_handle},
        reactive::{effect, on_cleanup, spawn},
    },
    run_loop, runtime,
};
use clankerdiff_server::{DiffServer, ServerOptions};
use std::{cell::RefCell, rc::Rc, sync::mpsc::Sender};
use tokio::sync::mpsc::unbounded_channel;

pub(crate) fn root(
    args: CliArgs,
    client: Option<DiffClient>,
    outcome: Option<Sender<Option<ReviewSubmission>>>,
    preferences: Preferences,
) -> impl View {
    let size = preferences.interface_size();
    let (send, mut outputs) = unbounded_channel();
    let view = DiffClientView::mount(
        preferences.theme(),
        DiffViewerOptions { view_mode: preferences.view_mode(), ..DiffViewerOptions::default() },
        move |output| {
            let _ = send.send(output);
        },
    );
    let theme_view = view.clone();
    let appearance = clankerdiff_rouge_shell::rouge::core::appearance();
    let prefs = preferences.clone();
    effect(move || {
        appearance.get();
        theme_view.review().set_theme(prefs.theme());
    });
    let events = view.clone();
    let task = spawn(async move {
        let (server, client) = match open_client(args, client).await {
            Ok(pair) => pair,
            Err(error) => {
                events.set_failure(error);
                return;
            }
        };
        let mut updates = client.subscribe();
        events.set_state(&updates.latest());
        let outcome = Rc::new(RefCell::new(outcome));
        let mut repository = None;
        let mut lookup = None;
        loop {
            tokio::select! {
                result = updates.changed() => {
                    let Ok(state) = result else { break };
                    events.set_state(&state);
                }
                output = outputs.recv() => {
                    let Some(output) = output else { break };
                    match output {
                        ReviewOutput::ThemeChanged(id) => preferences.remember_theme(&id),
                        ReviewOutput::ViewModeChanged(mode) => preferences.remember_mode(mode),
                        ReviewOutput::Event(DiffReviewEvent::SubmitReview(submission)) => {
                            if let Some(outcome) = outcome.borrow_mut().take() {
                                let _ = outcome.send(Some(submission));
                                run_loop::stop();
                                break;
                            }
                            if let Ok(json) = serde_json::to_string_pretty(&submission) { println!("{json}"); }
                        }
                        ReviewOutput::Event(DiffReviewEvent::Cancel) => { run_loop::stop(); break; }
                        ReviewOutput::Event(DiffReviewEvent::CopyFormattedReview(text)) => copy(text),
                        ReviewOutput::Event(DiffReviewEvent::Lookup(request)) => {
                            if let Some(old) = lookup.take() { cancel(&old); }
                            let (view, client) = (events.clone(), client.clone());
                            lookup = Some(spawn(async move {
                                let fallback = request.clone();
                                let reply = runtime::spawn(async move { client.lookup(request).await }).await
                                    .unwrap_or_else(|error| LookupReply::new(fallback, Err(error.to_string())));
                                view.lookup_finished(reply);
                            }));
                        }
                        ReviewOutput::Event(event) => {
                            let (view, client) = (events.clone(), client.clone());
                            repository = Some(spawn(async move {
                                let result = runtime::spawn(async move { client.handle(event).await.map_err(|error| error.to_string()) }).await
                                    .unwrap_or_else(|error| Err(error.to_string()));
                                view.command_finished(result);
                            }));
                        }
                    }
                }
            }
        }
        if let Some(task) = repository {
            task.cancel();
        }
        if let Some(task) = lookup {
            task.cancel();
        }
        drop(server);
    });
    on_cleanup(move || task.cancel());
    let handle = view.clone();
    let root = div()
        .size_full()
        .min_h(0.0)
        .flex()
        .col()
        .on_action(move |command: &RunCommand| {
            let _ = handle.handle_command(command.0.clone());
        })
        .child(view.view());
    size.bind(bind_shortcuts(root))
}

fn bind_shortcuts(root: Div) -> Div {
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
    .on_action(|_: &Quit| window_handle().quit())
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

fn cancel(task: &clankerdiff_rouge_shell::rouge::reactive::Task) {
    task.cancel();
}

pub(crate) fn copy(text: String) {
    clankerdiff_rouge_shell::rouge::window_handle().copy_text(text);
}
