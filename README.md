# Interoperability tests

Synthetic drawings saved by TraceDraw in every `.cdr` version (7 to 22),
and what another program reported when it opened them.

- `bundles/`: zips of test files; `bundles/latest.txt` names the newest.
- `results/`: zips with `results.log` (one line per file: `OK`, `ERR`,
  `CRASH` or `HANG`) and a picture of page 1 of each file that opened.

This branch holds test data only; it is not part of the TraceDraw sources.
