# Rustpad

Enkel notisblokk skrevet i Rust – i to utgaver:

- **`rustpad`** – terminalversjon (crossterm), minimal og rask
- **`rustpad-gui`** – grafisk versjon à la Notepad (eframe/egui)

## Funksjoner (GUI)

- Fil: Ny, Nytt vindu, Åpne, Lagre, Lagre som, Skriv ut (via `lp`/CUPS)
- Rediger: Angre/Gjør om, Klipp ut/Kopier/Lim inn, Finn/Erstatt, Gå til linje, Klokkeslett/dato
- Format: Ordbryting, skriftstørrelse
- Vis: Zoom, statuslinje, **Markdown-visning** (Ctrl+M) med ferdig rendret tekst
- Statuslinje med linje/kolonne, zoom, linjeskifttype og UTF-8
- Spør om lagring ved lukking med ulagrede endringer

## Hurtigtaster

| Tast | Handling |
|------|----------|
| Ctrl+N / Ctrl+Shift+N | Ny fil / nytt vindu |
| Ctrl+O | Åpne |
| Ctrl+S / Ctrl+Shift+S | Lagre / lagre som |
| Ctrl+P | Skriv ut |
| Ctrl+F / F3 / Shift+F3 | Finn / finn neste / forrige |
| Ctrl+H | Erstatt |
| Ctrl+G | Gå til linje |
| F5 | Sett inn klokkeslett/dato |
| Ctrl+M | Markdown-visning av/på |
| Ctrl+Q | Avslutt |

Terminalversjonen bruker ^S=lagre, ^P=markdown-forhåndsvisning, ^Q=avslutt.

## Bygg og installasjon

```sh
cargo install --path .
```

Installerer både `rustpad` og `rustpad-gui` til `~/.cargo/bin`.

Kjør med valgfritt filnavn: `rustpad-gui notat.md`

## Tester

```sh
cargo test --bin rustpad-gui
```
