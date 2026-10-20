# Legacy contracts (historical, unmaintained)

These scripts are historical source-shape checks from the M18-M22 checkpoints.

- They are **not maintained**.
- They are **excluded from the M15 acceptance gate**.
- Several of them assert symbols that predate M24, so they are **expected to fail**
  against the current tree. A failure here is not evidence of a regression.

They are retained only as development-history evidence: they record what each
milestone checkpoint asserted at the time it was accepted. Their repository-root
resolution has been kept pointed at the repository root so they still read the
current sources, which is why stale assertions surface as ordinary assertion
failures rather than path errors.

The active gate is:

```sh
./scripts/m15-full-automated.sh --profile full
```

Active scripts remain in `scripts/`; nothing in `scripts/legacy-contracts/` is
invoked by the release, install, or M15 acceptance paths.
