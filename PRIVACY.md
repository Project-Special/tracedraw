# Privacy policy

TraceDraw is a vector editor that runs on your own computer. This policy
covers the desktop program (installer, portable zip, Microsoft Store,
macOS and Linux packages), `tracedraw-cli` and the browser version.

## What TraceDraw collects

Nothing. TraceDraw has no accounts, no telemetry, no analytics, no crash
reporting and no advertising. It does not send your drawings, your files
or anything about you or your computer anywhere, except what you ask the
AI features to send to the AI service you set up yourself, and what you
save to GitHub with File > GitHub (see below).

## What stays on your computer

- Your drawings, where you save them.
- Preferences, the Shelf and thumbnails of recent drawings, in
  TraceDraw's preferences folder (in the browser version: the page's local
  storage in your browser). Deleting that folder removes them; Options >
  Global > Reset all settings puts the preferences back as they came.
- Automatic backups of open drawings, in the folder set in Options > Save
  (by default a `TraceDraw` folder in your system's temporary folder).
- AI Drawing's settings (service, address, model) with the preferences;
  your API key only if you tick "Remember the key on this computer". On
  Windows it is then stored encrypted for your Windows account; on macOS
  and Linux the preferences file can be read by your user only; in the
  browser version it is in the page's local storage. Otherwise the key is
  forgotten when TraceDraw closes. A GitHub token for File > GitHub is
  kept the same way, only if you tick "Remember the token on this
  computer".

## When TraceDraw uses the network

On its own, only to look for a new version:

- Once a day at start (Options > Global > General > "Check for updates
  at start", on unless you turn it off; never in the browser version),
  the desktop program asks GitHub (`api.github.com`) for the latest
  release of TraceDraw. The request carries only the program's version
  (`TraceDraw/<version>`); like any request, it shows GitHub your IP
  address. When you accept a new version, it is downloaded from GitHub
  and checked against the release's SHA-256 digest before it replaces the
  program.

Everything else only when you ask it to:

- Help, the project's pages and links you click open in your web
  browser, which then follows its own privacy policy.
- Send To > Mail hands a file to your mail program; you choose whether to
  send it.
- `tracedraw --mcp` talks over standard input and output with the
  program that started it, on your computer; it opens no network port.
- Options > Global > General > "Let AI agents on this computer edit the
  open drawing" (off unless you turn it on) opens a port on your
  computer only (`127.0.0.1`), guarded by a key kept in the preferences
  folder, for `tracedraw --mcp-window`; nothing outside your computer can
  connect.
- AI Drawing (Tools > AI Drawing), only when you press Draw: it sends
  your request, the earlier requests of the same conversation and a
  description of the active page (its size, layers and objects: types,
  positions, colours and text; no file names or pictures) to the AI
  service whose address you entered, with your API key. That service
  handles them under its own terms and privacy policy. TraceDraw has no
  AI service of its own and sends nothing to its authors.
- AI images (making, remixing or cutting out a picture with the same AI
  service), only when you ask: the prompt and, to remix or cut out, the
  picture you chose, as a PNG.
- File > GitHub, only when you use it: opens and saves the drawing in
  the GitHub repository you name, through GitHub's API, with your own
  token.

## The browser version

The browser version at <https://igtoth.github.io/tracedraw/> is a set of static
files served by GitHub Pages. TraceDraw itself sends nothing (AI Drawing
aside, as above, from your browser to the service you set up). When a
drawing's text needs a font of the page's font catalog, or the pointer
rests on one in a font list, the page fetches that font's files from
GitHub's file server (`raw.githubusercontent.com`). GitHub, as the host
of both, may log visits as described in the
[GitHub Privacy Statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).
Files you open in it stay in your browser.

## Contact

Questions about this policy: open an issue at
<https://github.com/igtoth/tracedraw/issues>.

Last updated: 2026-10-23.
