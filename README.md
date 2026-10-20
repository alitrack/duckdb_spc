# duckdb_spc — Statistical Process Control in DuckDB SQL

A DuckDB C-API extension (Rust) exposing the core of JASP's *Process
Capability Studies* and *Control Charts* as SQL functions. v0.1.0 covers
**process capability** and **control-chart limits** — the two slices that are
fully closed-form (no convergence) and trivially verifiable against a reference
oracle.

```sql
-- process capability (within σ via I-MR) + performance (overall sd)
SELECT spc_capability('20.1,19.8,20.0,...,20.0', '20.5', '19.5', '20.0');
-- -> {"cp":0.7004,"cpk":0.6584,"cpm":0.6949,"pp":0.9523,"ppk":0.8951,"ppm":4851.76,...}

-- control-chart limits
SELECT spc_chart('12.1,11.8,12.0,...', 'i_mr', 0);     -- I-MR individuals
SELECT spc_chart('30.1,29.8,30.0,29.9,30.2, 29.7,...', 'xbar_r', 5);  -- X̄-R, subgroup size 5
```

Both functions return a compact **JSON object** string; each metric is a
top-level key (missing metrics are JSON `null`).

## Functions

### `spc_capability(x VARCHAR, usl VARCHAR, lsl VARCHAR, target VARCHAR) → VARCHAR(JSON)`

Process capability / performance for one measurement stream.

- **within** σ = MR̄ / d₂(2) (I-MR individuals, d₂(2)=1.128)
- **overall** σ = sample standard deviation (ddof = 1)

| key | meaning |
|---|---|
| `cp` | (USL−LSL)/(6·σw) — needs both limits |
| `cpl` / `cpu` | (mean−LSL)/(3σw) / (USL−mean)/(3σw) |
| `cpk` | min of the present one-sided values |
| `cpm` | Bester/JMP target-based index — needs both limits + target |
| `pp` `ppl` `ppu` `ppk` `ppm` `pt` | same on the **overall** σ (process performance) |
| `pct_nc`, `ppm` | normal-approx nonconformance (overall σ) |
| `mean` `sd_within` `sd_overall` `mrbar` `n` | summary |

**Limits are strings**, not numbers: an absent limit/target is the empty
string `''` (or `NA` / `none` / `NULL` / `-`). This is because DuckDB auto-
propagates a SQL `NULL` input straight to the scalar output *without invoking
the function*, so `NULL` can't be the "absent" sentinel — an empty string can.

If neither `usl` nor `lsl` is given, the result is `NULL` (nothing to
compare against).

### `spc_chart(x VARCHAR, chart VARCHAR, k BIGINT) → VARCHAR(JSON)`

Control-chart central line and 3σ limits.

- `chart='i_mr'`: individuals + moving range.
  Keys `cl`, `i_ucl`, `i_lcl`, `mr_cl`, `mr_ucl`, `mr_lcl`.
- `chart='xbar_r'`: subgroups of size `k` (values are consecutive `k`-groups,
  row-major). Keys `xbar_cl`, `xbar_ucl`, `xbar_lcl`, `r_cl`, `r_ucl`, `r_lcl`.
  Constants A₂/D₃/D₄/d₂ from Montgomery *Introduction to Statistical Quality
  Control*, Table VII, for k = 2..10.

Returns `NULL` if `k` is out of range or doesn't divide the data length.

## Conventions

Matches JASP `jaspQualityControl` and Montgomery. `x` is a flat string of
measurements — commas or whitespace separated (`"20.1,19.8\n19.9 20.2"`).

## Build

```sh
# WSL, offline deps (rsproxy cache). Extension name must stay spc_ext:
# the C-API entry symbol (spc_ext_init_c_api) is derived from the filename.
export CARGO_NET_OFFLINE=true
mkdir -p configure
echo "linux_amd64"  > configure/platform.txt
echo "v0.1.0"       > configure/extension_version.txt
DUCKDB_EXTENSION_NAME=spc_ext DUCKDB_EXTENSION_MIN_DUCKDB_VERSION=v1.2.0 \
  cargo build --offline --release
python3 extension-ci-tools/scripts/append_extension_metadata.py \
  -l target/release/libspc_ext.so \
  -o target/release/spc_ext.duckdb_extension \
  -n spc_ext -dv v1.2.0 \
  -evf configure/extension_version.txt -pf configure/platform.txt
```

Load with an unsigned build: `duckdb -unsigned -c "LOAD 'target/release/spc_ext.duckdb_extension';"`.

## Test

```sh
cargo test --offline --lib   # 10 unit tests: math vs scipy oracle + sentinels
bash test/run_spc.sh         # 32 e2e checks through the built extension
```

`test/oracle.py` is the authoritative reference (scipy `norm` + Montgomery
constants) — regenerate `test/oracle_ref.json` with it after changing any
formula.

## Scope (v0.1.0)

In: capability (Cp/Cpk/Cpm/Pp/Pk/Ppm/PT + ppm), I-MR and X̄-R limits.

Out (v0.2.0+): attribute charts (p/np/c/u), time-weighted (EWMA/CUSUM),
rare-event charts, Cpk confidence intervals, subgroups-within capability
(R̄/ S̄), DOE (factorial/RSM), MSA (gauge R&R, linearity), acceptance
sampling. The `jaspQualityControl` source is the reference for all of them.

## License

MIT
