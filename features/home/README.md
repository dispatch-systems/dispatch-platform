# Home

A DSP's home page, where a DSP opens: each member's own, made of the widgets the features supply
(the `widgets` slot), placed where they choose on its grid. Mandatory, so every DSP has it, and
open to every member.

- **Its page** (`#dsp/<id>/overview`, "Home Page") is the DSP's landing page, which every
  member may open: it declares no permission and asks for none. Without a landing page, a
  DSP opens on the first page its member may open, or says it has none.
- **The grid:** twelve columns by seven rows filling the page below its header, edge to edge,
  about a desktop screen's shape, so each cell is nearly square; a layout is the same on every
  screen and the page never scrolls (`fills`). A widget sits at one of the size steps it declares, in the cells it was placed in;
  one placed in cells none of its steps fills shows at its nearest. A layout is kept only when
  each widget fits the grid, once, and a widget is offered to add only while it fits.
- **Where Home has no grid:** a window as narrow as one whose sidebar is a drawer (980px or less),
  or a DSP whose features give the member no widget, shows that the page is still being built.
- **A member's layout** (`backend/layout.rs`) is their preference `home.layout`: each widget they
  placed, by its id, column, row, width and height, kept by core and sent with their view.
  No widget is on it until they add one: a page never arranged is blank. A widget of a feature they don't
  have now keeps its place, and one back where another was placed meanwhile shows at the free
  cells nearest its own, or waits, kept, while there are none.
- **Arranging it:** Edit, or Add widget on a page with none, loads the editor: a widget picked up
  follows the pointer, and those in the cells it would land in move to the free cells nearest
  their own while it's held there, and back once it moves on (`moving`, worked out from where
  they were as it was picked up); one resized from any edge or corner snaps between its steps
  as the pointer goes, never over another; one removed goes; others are added from the list of
  those that fit; Reset clears the page. Each change shows at
  once, glides into place, and is kept behind it, made again on what another screen kept
  meanwhile.
- **A widget** loads its own code when first shown and fails alone, saying it couldn't load.
- **Upkeep:** none.
