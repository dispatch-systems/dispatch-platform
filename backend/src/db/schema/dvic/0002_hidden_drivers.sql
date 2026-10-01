-- Drivers kept out of this DSP's DVIC data. Only the operator's dvic-hide and dvic-unhide
-- commands change it and no route reads it; publication drops these drivers' rows
-- before writing any report copy or inspection.
CREATE TABLE IF NOT EXISTS dvic_hidden_drivers (
    transporter_id TEXT PRIMARY KEY,
    note TEXT NOT NULL,
    hidden_at TEXT NOT NULL
);
