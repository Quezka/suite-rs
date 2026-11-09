The shared look and behaviour of every Quezka app: Quire, Ligature, Gantry, Folio, Facet, Bivouac, Missive, Natter, Pillion, Cadence and Vesper, on the desktop and the phone. The Rust kit (`quezka-kit`), the Python kit (`quezka_ui`) and the phone themes all take their values from `tokens.json`; change a value there, never in one app.

## Principles

- **Big and clear first.** The main user doesn't see well. Body text is `body` (16px) and nothing anyone must read is smaller than `caption` (13px). Text meets 4.5:1 on its ground in every theme; the high-contrast themes are black and white with outlines, and never use glass.
- **Plain grounds, solid content, glass chrome.** Pages sit on `bg`, content on solid `surface` cards. Only chrome that floats over content (bars, the sidebar, the dock, menus, sheets) is glass: a real blur of what's under it, tinted with `surface` at `glass-tint`, a light rim along its top edge and `shadow-float`. Never a gradient wash or colour blobs behind anything.
- **One accent.** `accent` marks the one action and the chosen item on a screen; `accent-soft` with `on-accent-soft` fills chosen tabs, sidebar items and secondary buttons. `danger`, `success` and `warning` only mean their state and always come with a word or icon.
- **Corners nest.** Outer surfaces use `radius-lg` and sit `gap` apart and `gap` from the window edge. Anything sitting `space-3` inside one uses `radius-sm` (20 − 12); controls use `radius-md`; pills use `radius-pill`. Sidebar items use radius-lg minus the sidebar's 10px padding.

## Type

Set everything in Manrope (four weights shipped with each app). Use the styles by role: `headline` for page titles, `title-lg` for sheets and dialogs, `title` for card and row titles, `body` for text, `label` for buttons, tabs and chips, `caption` for small print, `display` only for the one big number of a screen. Weights: 400 text, 500 captions, 600 labels and titles, 700–800 headings. Never use letter-spacing except on uppercase section labels.

## Space and layout

- `gap` (10px) is the spacing between cards, between the sidebar and the page, and from the window's edges: the sidebar's bottom and the last card's bottom line up.
- Cards pad `space-3`; Settings cards pad `space-4`; dialogs pad `space-5`.
- A setting is a row: the title (`title`) and a hint under it (`body` in `muted`, wrapping, never cut off), the control on the right. On/off is always a switch, never a checkbox.
- Windows fit a 1366×768 laptop at 100% and scale with the interface size setting. A page taller than the window scrolls; nothing is clipped.

## Words

- Plain, friendly English (with Italian and Russian kept in step): what people recognise, not how it's built. "Turn on Bluetooth", not "Enable BLE adapter".
- Buttons say what happens: Save, Delete, Go here. The action that follows says it happened: "Saved", "Note deleted · Undo".
- Errors say what went wrong and what to do: "Classeviva didn't accept that username and password."
- Every string is translatable; layouts must survive Russian, which runs about a third longer than English: hand-drawn words shrink a little, then end with "…", never run off an edge.

## Behaviour

- **Undo, not "Are you sure?"** Deleting acts at once and shows a snackbar with Undo for 4 seconds. Ask first only for things that can't be undone (wiping data, signing out of sync).
- **Swipes are for moving around.** On phones, horizontal swipes change section; a destructive swipe on a list row needs the snackbar's Undo.
- **Enter leaves a one-line box**; Ctrl+Enter saves a dialog; Escape closes it.
- **Motion is short and quiet:** 90ms for a press, 180ms for a move (tab pill, switch knob, segment), 220ms for a sheet; ease-out. Nothing loops, nothing bounces; with reduced motion, changes are instant.
- **Phones:** content runs edge to edge under the status and navigation bars; words and buttons stay inside the safe area; text fields follow the keyboard's own cursor (space-bar slide) and its autocorrect.

## Iconography

Material Symbols, outlined (Apache 2.0), filled when chosen. 20–24px beside words, tinted with the words' colour. No emoji as icons.
