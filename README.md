<p align="center"><img src="src-tauri/icons/128x128@2x.png" width="128" height="128" alt=""></p>

# Sheafly

**English** · [Tiếng Việt](README.vi.md)

A Microsoft Project–style scheduler that runs on your own computer (macOS, Windows, Linux), in English or Vietnamese. Built with Tauri 2, React 19 and a scheduling engine written in Rust. Open source under the MIT license; your projects stay on your machine, with no account or server.

**[Download the latest release](https://github.com/hoanghainh1188/sheafly/releases/latest)**

![Sheafly: task sheet, Gantt chart, task details and the EVM panel](docs/images/screenshot-en.jpg)

## Features

- **Scheduling:** critical path (CPM) from FS/SS/FF/SF links with lag; constraints (As Soon As Possible, As Late As Possible, Start/Finish No Earlier/Later Than, Must Start/Finish On); deadlines; manually scheduled tasks; split tasks; recurring tasks.
- **Editing:** copy and paste tasks (⌘C / ⌘V on selected rows, with their subtasks, assignments and the links between them); Shift+click selects a range of rows and ⌘D fills the column of the focused cell down from the first row; drag one segment of a split task on the Gantt.
- **Calendars:** work weeks, nonworking days and exceptions; a project calendar plus calendars for individual tasks and resources.
- **Resources:** people and equipment, max units, rates, task types (Fixed Units, Fixed Duration, Fixed Work), overallocation found from overlapping hours, availability by date (leave, half time) that overallocation and leveling follow, resource leveling in an order you set (up to 8 criteria: start, priority, slack, deadline, duration, ID, custom fields), urgent tasks marked "Interrupts" that leveling lets go ahead of work in progress ranked lower on the same people (that work is split and resumes after; the Gantt draws those pauses dashed), a useful-productivity factor for the project and per person (for example 80%, the time left after personal time, fatigue and delays) that slows the rate of work, so durations, usage and the load map plan on useful hours; roles and skills on resources, with an optional scale of levels (such as Junior, Mid, Senior) per skill, and ordered assignment rules from custom field values to a person or to whoever has a skill at a minimum level (Project → Assignment Rules…); Assign by Rules… previews who should take each unassigned task, ranked by who would finish it first next to their load, shows what assigning and leveling would then do (tasks that move, old and new dates, deadlines missed, people still overallocated) and applies that plan in one undo step.
- **Tracking:** baselines 0–10, % complete, actual start and finish that drive the schedule the way Project does, moving unfinished work past the status date, late flags, costs.
- **Reports:** earned value (PV, EV, AC, SPI, CPI, EAC…), burn-down and S-curve.
- **Views:** seven tabs (Gantt, Tracking, Resources, Usage, Load, Forecast, Network diagram); Load is a heatmap of each person's work against their capacity by day or week (click a person to see their tasks); Forecast simulates the project a thousand times from each task's low and high effort (with tasks that slip together) and shows the chance of finishing by each date and the tasks most at risk; day, week and month timescales; filter, group and highlight per view, including tasks outside a span (finishing after a date or milestone you pick, or after their deadline); PDF export.
- **Languages:** English and Vietnamese. The first launch follows your system language; switch any time in View → Language. English uses ISO dates (`2026-01-05 08:00`) and Microsoft Project terminology.
- **Interface:** a toolbar grouped by colour with icons (history, create, selected task, project, reports), and disabled commands that say why. Works from the keyboard: arrow keys move between tabs; on the Gantt, ↑ ↓ select rows, Enter edits, Option+← → (Alt on Windows and Linux) move a task by a working day; Esc closes dialogs. Light and dark themes: View → Appearance follows the system or stays light or dark.
- **Task trackers:** File → Import → Pull from Task Tracker brings issues from a Backlog or Jira Cloud project: set the site, project key and a filter, fetch with progress (and a Stop button), see what would be added, changed or marked gone from the source (with problems by issue), then apply it in one undo step. A repeated pull with no change at the source changes nothing, and a pull never overwrites an edit you made in Sheafly when the source did not change that field. When the same field changed on both sides since the last sync, to different values, the pull lists it as a conflict and applies nothing until you choose, field by field or for all, to take the source's value or keep Sheafly's. Issues map to tasks by key (effort, start and due dates, status as progress, parents as groups, custom fields of the same name, assignees matched by the email on each resource, set in the resource sheet). The API key lives in your operating system's credential store (Keychain, Credential Manager, Secret Service), never in the project file; the app talks only to `*.backlog.com`, `*.backlog.jp`, `*.backlogtool.com` and `*.atlassian.net`. Jira signs in with an Atlassian email and an API token (also kept only in the credential store), filters with a JQL condition, takes the original estimate or a story point field as effort, a start date only from a date field you name, and finds assignees by email or, when Jira hides it, by display name. File → Export → Push Dates to Task Tracker writes the dates Sheafly computed (and, if you ask, the assignee) back to those issues: choose what to write, preview exactly what will change at the source, and only then press the button that writes. By default only empty fields at the source are filled; nothing is ever cleared; the days that went through are remembered, so a pull right after a push does not read them back as constraints or deadlines. Sheafly cannot undo a change at the source.
- **Files:** `.sheafly` (SQLite; files from ProjectDesk, `.pdproj`, open as they are) keeps the undo history with the project. Double-click a project file to open it in Sheafly; File → Open Recent lists the last ten. Saves are crash-safe, unsaved work is autosaved and offered back after a crash, and partly damaged files still open. Import and export MSPDI (`.xml`) to exchange projects with Microsoft Project; `.mpp` files cannot be read. File → Import Table brings people or tasks in from a CSV or Excel/OpenDocument sheet: map each column to a field (guessed from its header), see every cell that cannot be read at its line in the file, and import it all in one undo step.

## Install

Download an installer from [Releases](https://github.com/hoanghainh1188/sheafly/releases): `.dmg` for macOS (Apple Silicon and Intel, from 1.2.0; earlier releases are Apple Silicon only), `.msi` or `-setup.exe` for Windows, `.AppImage`, `.deb` or `.rpm` for Linux.

The installers are not signed with a developer certificate yet (the macOS app has only an ad-hoc signature). On macOS, try to open Sheafly once, then go to **System Settings → Privacy & Security** and click **Open Anyway** near the bottom (since macOS 15, right-click → Open no longer works for unsigned apps). If macOS says the app is damaged (releases before the ad-hoc signature), run:

```bash
xattr -dr com.apple.quarantine /Applications/Sheafly.app
```

On Windows, SmartScreen may ask first; choose **More info → Run anyway**.

Each release also has `SHA256SUMS.txt`. To check a download, run `shasum -a 256 <file>` (macOS, Linux) or `certutil -hashfile <file> SHA256` (Windows) and compare the result with its line there.

## Build from source

You need [Rust](https://rustup.rs) (stable), Node.js 22 (or 20.19 and later) and [pnpm](https://pnpm.io). Linux also needs Tauri's system packages (see the [Tauri prerequisites](https://tauri.app/start/prerequisites/)).

```bash
pnpm install
pnpm tauri dev      # run the app while developing
pnpm tauri build    # build installers into target/release/bundle
```

The interface only works inside the Tauri window; opening `localhost:1420` in a normal browser cannot load a project.

## Tests

```bash
cargo test -p sheafly-engine      # scheduling engine
cargo test -p sheafly             # Tauri shell
pnpm exec tsc --noEmit                # type check
pnpm test                             # interface (Vitest)
cargo bench -p sheafly-engine --bench large_project              # 1,000 tasks, 10,000 commands
cargo bench -p sheafly-engine --bench large_project -- leveling  # leveling 500 tasks
```

## Project layout

- `crates/engine`: the scheduling engine, with no Tauri dependency. Every date, slack value, critical path, cost and EVM figure is computed here.
- `src-tauri`: the Tauri shell: native menu, file dialogs, autosave, settings.
- `src`: the React interface. It only displays what the engine returns. Text lives in typed message catalogs (`*.i18n.ts`).
- `.cursor/plans/projectdesk_macos_da6c3154.plan.md`: the plan (in Vietnamese), with the decisions behind each phase and the roadmap.
- `CLAUDE.md`: architecture notes for people (and tools) changing the code.

## Known limitations

- `.mpp` files cannot be read; use MSPDI (`.xml`) to exchange projects with Project.
- The installers are not signed or notarized yet.
- Task tracker sync is Backlog and Jira Cloud (Jira has not been tried on a real site yet), and pulling and pushing are separate actions. Only dates and the assignee are written back; names, effort and status are not. Asana comes later.
- One user, one window.
- English and Vietnamese only.

## License

MIT. The app bundles open-source packages and the Be Vietnam Pro font (SIL Open Font License); their copyright notices and license texts are in [`src-tauri/THIRD_PARTY_LICENSES.txt`](src-tauri/THIRD_PARTY_LICENSES.txt), which every installer includes and Help → Third-Party Licenses opens. Regenerate it with `node scripts/third-party-licenses.mjs` after changing dependencies; CI checks that it is current.

## Trademarks

Microsoft Project and Windows are trademarks of Microsoft Corporation; macOS is a trademark of Apple Inc.; Jira is a trademark of Atlassian; Asana is a trademark of Asana, Inc.; Backlog is a trademark of Nulab Inc. Other names belong to their owners. They are mentioned only to describe compatibility. Sheafly is independent software, not affiliated with, sponsored or endorsed by any of them.
