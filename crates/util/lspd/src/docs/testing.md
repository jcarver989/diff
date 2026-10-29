Temporary projects and in-process daemons for integration tests against language servers.

- [`TestWorkspace`] creates temporary projects that are removed when dropped.
- [`TestDaemon`] runs a daemon in-process with deterministic shutdown. [`TestDaemon::builder`] overrides timeouts, pauses the daemon's clock and installs [`TestGate`]s at [`TestPoint`]s.
- [`TestDaemonBuilder::fake_server`] serves the daemon's language with the shared fake language server. It holds diagnostics for files containing `// hold-diagnostics` until a `fake/releaseDiagnostics` request, and reports itself as loading while the workspace root holds a `.fake-loading` file, until a `fake/finishLoading` request. A `fake/reregisterWatchers` request makes it register its file watchers under a new id and unregister the old one.
