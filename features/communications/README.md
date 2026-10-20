# Communications

TODO(scaffold): what Communications is for a DSP, and the data it shows, in a few lines. Write down any rule that
isn't obvious from the code.

- **Switch:** `communications`, off for every DSP until the platform owner switches it on.
- **Permissions:** `communications.view`. Only owners hold them until a role grants them.
- **API:** `GET /api/dsp/communications`, behind `communications.view`.
- **Data:** `communications_items`, in each DSP's own database.
