# Golden fixture oracle

Regenerates `fixtures/*.json` by executing the **original upstream TypeScript
engine**, so the Rust port can be diffed against it directly.

```sh
./tools/oracle/regenerate.sh
```

Requires Node 22+ (for `--experimental-strip-types`) and the upstream repo
cloned at `./upstream`.

You should not normally need to run this. The fixtures are committed, and
regenerating them is only correct when you are *deliberately* changing the
ranking algorithm — in which case every existing user's rank moves, and that
should be a conscious, announced decision.
