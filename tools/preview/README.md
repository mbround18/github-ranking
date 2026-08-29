# Card preview

Renders every tier against a pair of themes into one PNG, and prints the WCAG
contrast ratio for each combination.

```sh
cd tools/preview && cargo run --release
```

Writes `sheet.png`. Edit `themes` in `src/main.rs` to check a different pair.

This exists because two real contrast bugs shipped past the unit tests and were
only obvious once rendered: Gold's near-white accent on the light theme, and
Iron's three greys on the dark one. Look at the cards after changing the design.
