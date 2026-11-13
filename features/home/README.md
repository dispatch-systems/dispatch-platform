# Home

A DSP's home page, where a DSP opens: each member's own, made of the widgets the features supply
(the `widgets` slot), placed where they choose on its grid. Mandatory, so every DSP has it, and
open to every member.

- **Its page** (`#dsp/<id>/overview`, "Home Page") is the DSP's landing page, which every
  member may open: it declares no permission and asks for none. Without a landing page, a
  DSP opens on the first page its member may open, or says it has none.
- **The grid:** twelve columns that stretch with the window, each row as tall as a column is
  wide, so a layout is the same on every screen. A widget sits at one of the size steps it
  declares, in the cells it was placed in; one placed in cells none of its steps fills shows at
  its nearest.
- **Where Home has no grid:** a window as narrow as one whose sidebar is a drawer (980px or less),
  or a DSP whose features give the member no widget, shows that the page is still being built.
- **A member's layout** (`backend/layout.rs`) is their preference `home.layout`: each widget they
  placed, by its id, column, row, width and height, kept by core and sent with their view.
  Until they arrange it, the page shows the widgets that have a place on one never arranged
  (`home`), in that order, and nothing is kept. A widget of a feature they don't have now keeps
  its place, and one back where another was placed meanwhile shows where it first fits. A layout
  is kept only when each widget fits the grid, once.
- **Arranging it:** Edit, or Add widget on a page with none, loads the editor
  (`react-grid-layout`, only then): each widget dragged anywhere on the grid, never over another,
  resized between its steps, or removed; others added from the list of those the member may add;
  Reset clears the layout back to where the widgets start. Each change shows at once and is kept
  behind it, made again on what another screen kept meanwhile.
- **A widget** loads its own code when first shown and fails alone, saying it couldn't load.
- **Upkeep:** none.
