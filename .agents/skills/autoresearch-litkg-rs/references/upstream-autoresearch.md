# Upstream Autoresearch Notes

Source repository:

- [karpathy/autoresearch](https://github.com/karpathy/autoresearch)

For comparison with other public autoresearch repos and the rationale for what
this repo does and does not borrow from them, see
[repo-comparison.md](repo-comparison.md).

Key upstream files:

- [README.md](https://github.com/karpathy/autoresearch/blob/master/README.md)
- [program.md](https://github.com/karpathy/autoresearch/blob/master/program.md)
- [prepare.py](https://github.com/karpathy/autoresearch/blob/master/prepare.py)
- [train.py](https://github.com/karpathy/autoresearch/blob/master/train.py)

## Core Upstream Mechanics

Upstream autoresearch is built around a very small research loop:

1. freeze one evaluation harness
2. edit one mutable program surface
3. run one experiment
4. extract one primary metric
5. keep or discard the change
6. log the outcome in `results.tsv`

The important structural ideas are:

- a tiny mutable surface
- a fixed, trusted evaluator
- comparable experiments
- lightweight experiment logging
- autonomous iteration without waiting for the human after setup

## Why A Direct Port Does Not Fit litkg-rs

`litkg-rs` is not a single-file training harness:

- it has multiple crates and multiple valid evaluation surfaces
- it owns benchmark metadata and autoresearch-target composition directly
- it has source-of-truth docs that must stay aligned with operator contracts
- it forbids destructive git rollback flows
- it uses `.agents` backlog files and the internal DB as repo memory

Because of that, the local adaptation changes four major things:

1. The loop is bounded.
2. The mutable surface is declared per run.
3. Trial branches replace destructive resets.
4. The research brief freezes both metrics and benchmark inputs.

## Mapping To litkg-rs

Upstream concept -> litkg-rs adaptation

- `prepare.py` immutable harness -> frozen evaluation commands plus frozen docs
  and benchmark inputs
- `program.md` mutable surface -> a declared crate/module/config slice
- training metric -> task-specific primary metric
- `results.tsv` -> `.logs/autoresearch/<tag>/results.tsv`
- infinite loop -> explicit run budget

## Typical Local Metrics

- `make benchmark-validate` pass/fail
- successful `render-autoresearch-target` execution for a frozen target id
- targeted or full `cargo test` pass/fail
- deterministic output shape or diff checks for generated artifacts
- explicit review rubric over rendered benchmark-driven target quality
