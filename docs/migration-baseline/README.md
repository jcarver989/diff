# GPUI behavioral baseline

This is the Step 1 acceptance record for the [hard-cutover migration](../../migration-plan.md). It records the existing production implementation; it does not add a renderer, compatibility layer, alternate frontend, or Rouge dependency.

## Revision and status

| Input | Recorded revision |
| --- | --- |
| Diff before renderer changes | `09a11f7e7fbaa354a2df4743b4ee18ede000a30c` |
| Local Rouge, inspected only | `4918b374d1f0574670bc738f3db3e9d4d346ac84` |
| j-terminal, clean tracked worktree | `ffff8c48904cd6a147c726512824c1c2666cd421` |
| aether, clean tracked worktree | `93394969aa5acad689e7cefc00400c5ce79b7aa3` |

Recorded on 2026-11-10. Renderer/host sources match the Diff revision above. Only acceptance tooling, fixtures, documentation, and the core example registration were added. Neither Rouge nor the sibling consumers was edited.

**Recorded:** source/API inventory, desktop/browser behavior checklist, all 265 explicit renderer/host test mappings, actual consumer usage, deterministic domain fixtures, eight browser screenshots, and browser startup/memory/interaction measurements. Existing renderer, host, core, and real-browser suites pass, with one existing ignored input test.

**Not signed off:** native release startup/RSS/frame timing, native OS screenshots/menu/clipboard/IME behavior, macOS behavior, and hardware-WebGPU or other-browser coverage. Browser timings are boundary measurements, not native/GPU performance results. These remain explicit pre-deletion capture/signoff items; Step 1 must not be treated as full native/platform acceptance. See [remaining captures](#remaining-native-and-platform-captures).

## Acceptance artifacts

| Artifact | Purpose |
| --- | --- |
| [contract.md](contract.md) | Required behavior and public host boundaries; checklist for rewrite acceptance |
| [consumers.md](consumers.md) | Concrete shared UI/shell and Diff usage in sibling apps |
| [api.json](api.json) | Public declarations/reexports in 62 source files, baseline file SHA-256s and line locations |
| [test-map.json](test-map.json) | 265 named tests, source hashes and planned Rouge integration-test destinations |
| [consumers.json](consumers.json) | Consumer revision, dirty-state and explicit import/dependency evidence |
| [system.json](system.json) | Actual invoked tool versions, CPU/RAM and physical Vulkan devices |
| [browser/results.json](browser/results.json) | Raw samples, exact build asset/fixture/image hashes, capture-script hash, environment and limitations |
| [browser/observed-events.json](browser/observed-events.json) | Actual document acknowledgements, command results, review copy/submit payloads |
| [browser/console.json](browser/console.json) | Graphics backend selection and warnings, not discarded from the record |
| [browser/image-validation.json](browser/image-validation.json) | Image dimensions, color counts and entropy; nonblank/distinct-image checks, not human visual approval |
| [screenshots.md](screenshots.md) | Browser screenshot gallery and state descriptions |
| [validation.json](validation.json) | Executed commands, outcomes, and known ignored test |
| [tests/fixtures/migration](../../tests/fixtures/migration) | Small/refresh/empty documents, structured feedback, Markdown, and row-count manifest |

The source inventory is lexical, not a compiler-resolved public API or a signature-compatibility promise. It includes public methods in private implementation modules, root reexports/aliases, public modules, and the UI prelude. Macro-only APIs are not expanded; action names reexported at the root are listed there. Exact source remains recoverable with `git show <revision>:<path>`, including fields, attributes, macro definitions and generic signatures. The rewrite preserves **behavior**, not GPUI types.

No test has been approved for deletion. `required_not_yet_ported` means a future port is required, not that the test already exists in Rouge. The 265 explicit tests comprise 151 Diff renderer, 77 UI, 3 shell, 16 desktop, and 18 web tests (including two TypeScript browser tests). Doctests are executed where available but are not counted in this source-name map. Parameterized cases stay under their named test.

## Reproduce

Run from the repository root, before the structural cutover. Rust, the WASM target, Trunk, Node and the web crate's Playwright Chromium installation are prerequisites. Install the web dependencies using its lockfile if necessary (`cd crates/diff/web && npm ci && npx playwright install chromium`). Do not replace the application with a prototype to reproduce these results.

```sh
python3 scripts/migration/inventory.py --check
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tests/migration -p '*_test.py' -v

cargo run -p clankerdiff-core --features test-support \
  --example migration_fixtures -- target/migration-baseline/fixtures
cargo test -p clankerdiff-core --features test-support
cargo test -p clankerdiff-gpui --features test-support
cargo test -p clankerdiff-gpui-ui -p clankerdiff-gpui-shell \
  -p clankerdiff-gpui-desktop -p clankerdiff-gpui-web --all-features
cargo clippy -p clankerdiff-core --features test-support \
  --example migration_fixtures --test migration_fixture_test -- -D warnings

(cd crates/diff/web && trunk build --release && npx vitest run --browser.enabled)
node scripts/migration/capture-browser.mjs
```

The capture serves the existing `crates/diff/web/dist` over a temporary loopback server and closes its dedicated Chromium process tree afterward. It does **not** build the app; build it first. `window.wasmBindings` is the current Trunk initializer's binding name. For more samples, set `BASELINE_SAMPLES=20`. Optional positional arguments select an output directory and generated-fixture directory. Default output is ignored `target/migration-baseline/browser`; it never overwrites these curated artifacts automatically.

Regenerating `inventory.py` without `--check` deliberately rereads the live sibling worktrees; review changes to `consumers.json`. `--check` checks the immutable API/test snapshots from Git, not current consumer revisions. The tests exercise the CLI, exact source hashes, failure on changed inventory, and curated artifact hashes.

Large JSON documents are generated, not checked in: 100,001 rows, 100 files/6,100 rows, and very long Unicode/tab-containing lines. The small replacement changes content without changing either layout's row count. Fixture JSON regenerated byte-for-byte during validation; `manifest.json` records file and row counts. The two new core integration tests cover same-count presentation changes, full source availability/Unicode, file edge cases, valid comment anchors, and exact structured/formatted review export through public APIs.

Build output, browser dependencies, large fixtures and raw local command logs stay in ignored `target/`/`dist`/`node_modules`, not in the baseline. Git history, recipes, hashes and curated evidence replace any need to retain a second implementation after cutover.

## Recorded browser measurements

Release WASM, Chromium `156.0.8078.4`, 1280×800 CSS pixels, DPR 1. Host: Ryzen 9 9900X, 24 logical CPUs, approximately 30 GiB RAM, Linux/Arch. The physical GPU is an RX 7900 XT; **this headless run actually selected WebGL2/ANGLE SwiftShader** after no usable WebGPU adapter was found. `navigator.gpu` existing is not evidence of a usable adapter. See the full console and system records.

One fresh-context navigation-to-ready capture, including two animation frames and a fixed 150 ms settle: **735.7 ms**. OS caches were not cleared and this is not a native cold-start benchmark.

Medians of five samples, milliseconds:

| Fixture | Layout command boundary | Scroll command boundary | Changed-content refresh boundary |
| --- | ---: | ---: | ---: |
| Small, 17 unified rows | 117.7 | 121.3 | 123.4 |
| Large, 100,001 rows | 135.1 | 105.2 | 382.1 |
| Many files, 6,100 rows | 111.3 | 113.6 | 119.3 |
| Long lines, 3 unified rows | 113.0 | 112.8 | 149.1 |

These include command/JSON work plus two `requestAnimationFrame` callbacks, not isolated layout CPU time, frame rate, or GPU presentation. Small/long-line scroll requests may clamp without moving because the document fits. Layout alternates split/unified; scrolling requests 25 lines; refresh alternates changed added-line content at the same row count (and matching source text when supplied). The small case alternates the checked-in small/refresh pair.

Memory is sampled once per fixture after a fixed one-second settle, before that fixture's five operations:

| Fixture | JS heap used, MiB | WASM linear memory, MiB | Chromium process-tree RSS, MiB |
| --- | ---: | ---: | ---: |
| Small | 4.8 | 89.1 | 786.2 |
| Large | 95.0 | 141.3 | 974.0 |
| Many files | 100.1 | 177.1 | 1138.7 |
| Long lines | 119.5 | 177.1 | 1148.0 |

RSS sums dedicated Chromium process RSS, double-counts shared pages, and is not native Diff memory. Fixtures run in the recorded order in one context, without forced GC or resetting the WASM heap, so later samples include prior allocation high-water effects. These are settled snapshots, **not proof of long-lived steady state or independent per-document footprint**. Compare future runs using the same method/order/backend; do not use these few samples as hard performance thresholds.

Warnings include unavailable WebGPU, disabled subpixel AA without dual-source blending, resize polling timeout, and screenshot `ReadPixels` stalls. No page errors or error-level console messages were observed; warnings remain in the record.

## Remaining native and platform captures

Do not infer these from passing GPUI test windows or the browser screenshots:

- [ ] Native release startup-to-first-visible-content and settled whole-process RSS on the recorded hardware, with repeated samples.
- [ ] Native scrolling/layout-toggle/same-count-refresh CPU/frame measurements and mounted-row bounds on large documents.
- [ ] Native OS screenshots for the gallery states, exact window dimensions/display scaling, window chrome, menus and close/submit outcomes.
- [ ] Real native clipboard permissions/contents, IME composition, focus transfer, resizing, display-scale changes and macOS behavior.
- [ ] Hardware WebGPU, Firefox/Safari and constrained browser/embedded-host validation where product support requires them. Only Chromium/WebGL2 was observed here; this is not a newly narrowed support policy.
- [ ] Human visual review of the screenshot gallery. Image entropy/hash checks do not approve typography, clipping or color accuracy.

There is no existing desktop screenshot/benchmark CLI or exposed script option. The shell's optional script runner is not wired into the desktop app. Rather than invent another frontend or report old debug binaries as release measurements, those captures are explicitly outstanding.

Native follow-up procedure (no application changes required for manual captures):

1. Materialize a fixed local Git repository from the same old/new source cases; record its commit, `git status --porcelain`, `git diff --binary` and `git diff --cached --binary`, including untracked/binary content hashes. Use the recorded large-file/many-file recipes for scale cases. Do not benchmark this changing implementation worktree.
2. Run `cargo build --release -p clankerdiff-gpui-desktop`, record executable hash and actual toolchain, then launch `target/release/clankerdiff-gpui-desktop /absolute/fixture/repository --both`. Build time is not application startup.
3. Fix display backend, physical GPU, window size, theme and interface size. Record five process-launch-to-visible-content samples using an external window/capture tool and its timing uncertainty; there is no built-in native readiness signal. Sample `/proc/<app-pid>/status` `VmRSS` at documented idle intervals and after repeated scroll/resize/refresh, not Cargo's PID. `/usr/bin/time -v` gives maximum RSS at exit, not steady-state RSS.
4. Exercise the [contract checklist](contract.md) using the actual app, capture OS screenshots through an available compositor-approved tool, and record backend/DPI/output geometry with each image. Wayland capture/input permissions may require interactive approval; macOS requires separate hardware.
5. Use a native profiler for CPU/layout/frame claims, documenting profiler overhead and matching actions to fixtures. Attach raw samples and update the outstanding checklist before calling native acceptance complete.
