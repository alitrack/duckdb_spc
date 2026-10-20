-- duckdb_spc v0.1.0 end-to-end test (DuckDB-native, no Postgres DO blocks).
-- Each check emits (label, ok). The final row reports n_fail; the runner
-- (test/run_spc.sh) asserts n_fail = 0 and exits non-zero otherwise.
-- Tolerance: 1e-6 relative (ppm/pct_nc: 1e-5 relative).
-- Works on DuckDB 2.0-alpha CLI (json_extract returns JSON -> cast DOUBLE).

LOAD '/mnt/d/wsl2/duckdb_spc/target/release/spc_ext.duckdb_extension';

CREATE OR REPLACE MACRO num(j, key) AS json_extract(j, '$.' || key)::DOUBLE;
CREATE OR REPLACE MACRO ok(a, r, tol) AS
  CASE WHEN a IS NULL OR r IS NULL THEN false
       WHEN ABS(a - r) <= tol * GREATEST(1.0, ABS(r)) THEN true ELSE false END;

CREATE OR REPLACE TEMP TABLE cap_ts   AS SELECT spc_capability('20.1,19.8,20.0,19.9,20.2,19.7,20.0,20.1,19.9,20.0,20.3,19.6,20.0,20.1,19.8,20.0,19.9,20.2,19.8,20.0', '20.5', '19.5', '20.0') AS j;
CREATE OR REPLACE TEMP TABLE cap_up   AS SELECT spc_capability('5.0,5.2,4.9,5.1,5.3,4.8,5.0,5.2,5.0,5.1,5.4,4.7', '5.5', '', '') AS j;
CREATE OR REPLACE TEMP TABLE cap_oc   AS SELECT spc_capability('8.0,9.0,8.5,9.5,8.2,9.8,8.8,9.2,8.6,9.4,8.4,9.6', '9.0', '7.0', '8.0') AS j;
CREATE OR REPLACE TEMP TABLE chart_i  AS SELECT spc_chart('12.1,11.8,12.0,12.3,11.9,12.0,12.2,11.7,12.1,11.9,12.0,12.4,11.8,12.1,12.0,11.9,12.2,12.0,11.8,12.1,12.0,11.9,12.3,12.0', 'i_mr', 0) AS j;
CREATE OR REPLACE TEMP TABLE chart_x5 AS SELECT spc_chart('30.1,29.8,30.0,29.9,30.2,29.7,30.1,30.0,29.9,30.2,30.3,29.6,30.0,30.1,29.8,30.0,29.9,30.2,29.8,30.0,29.9,30.1,29.7,30.2,30.0,30.1,29.8,30.0,29.9,30.2', 'xbar_r', 5) AS j;
CREATE OR REPLACE TEMP TABLE chart_x4 AS SELECT spc_chart('41.0,40.5,41.2,40.8,40.9,41.1,40.7,41.0,40.6,41.0,40.9,40.8,41.1,40.7,40.9,41.0', 'xbar_r', 4) AS j;

CREATE OR REPLACE TEMP TABLE checks AS
  SELECT 'cap_ts.mean' AS label, ok(num((SELECT j FROM cap_ts),'mean'), 19.97, 1e-6) AS p
  UNION ALL SELECT 'cap_ts.cp',    ok(num((SELECT j FROM cap_ts),'cp'),   0.7003921568627439, 1e-6)
  UNION ALL SELECT 'cap_ts.cpl',   ok(num((SELECT j FROM cap_ts),'cpl'),  0.6583686274509777, 1e-6)
  UNION ALL SELECT 'cap_ts.cpu',   ok(num((SELECT j FROM cap_ts),'cpu'),  0.7424156862745102, 1e-6)
  UNION ALL SELECT 'cap_ts.cpk',   ok(num((SELECT j FROM cap_ts),'cpk'),  0.6583686274509777, 1e-6)
  UNION ALL SELECT 'cap_ts.cpm',   ok(num((SELECT j FROM cap_ts),'cpm'),  0.69489169406063, 1e-6)
  UNION ALL SELECT 'cap_ts.pp',    ok(num((SELECT j FROM cap_ts),'pp'),   0.9522786723020039, 1e-6)
  UNION ALL SELECT 'cap_ts.ppk',   ok(num((SELECT j FROM cap_ts),'ppk'),  0.8951419519638815, 1e-6)
  UNION ALL SELECT 'cap_ts.ppm',   ok(num((SELECT j FROM cap_ts),'ppm'),  4851.760316213353, 1e-5)
  UNION ALL SELECT 'cap_ts.pct',   ok(num((SELECT j FROM cap_ts),'pct_nc'), 0.48517603162133527, 1e-5)
  UNION ALL SELECT 'cap_ts.sd_w',  ok(num((SELECT j FROM cap_ts),'sd_within'), 0.23796192609182573, 1e-6)
  UNION ALL SELECT 'cap_up.cpnull', (num((SELECT j FROM cap_up),'cp') IS NULL)
  UNION ALL SELECT 'cap_up.cpu',   ok(num((SELECT j FROM cap_up),'cpu'),  0.5892688172043006, 1e-6)
  UNION ALL SELECT 'cap_up.cpk',   ok(num((SELECT j FROM cap_up),'cpk'),  0.5892688172043006, 1e-6)
  UNION ALL SELECT 'cap_up.ppm',   ok(num((SELECT j FROM cap_up),'ppm'),  14419.396813245112, 1e-5)
  UNION ALL SELECT 'cap_oc.cpk',   ok(num((SELECT j FROM cap_oc),'cpk'),  0.033141025641025876, 1e-6)
  UNION ALL SELECT 'cap_oc.ppk',   ok(num((SELECT j FROM cap_oc),'ppk'),  0.04715760341752041, 1e-6)
  UNION ALL SELECT 'cap_oc.ppm',   ok(num((SELECT j FROM cap_oc),'ppm'),  444317.4316642531, 1e-5)
  UNION ALL SELECT 'i_mr.cl',      ok(num((SELECT j FROM chart_i),'cl'),     12.020833333333334, 1e-6)
  UNION ALL SELECT 'i_mr.i_ucl',   ok(num((SELECT j FROM chart_i),'i_ucl'),  12.726198735738514, 1e-6)
  UNION ALL SELECT 'i_mr.mr_ucl',  ok(num((SELECT j FROM chart_i),'mr_ucl'), 0.8664652173913033, 1e-6)
  UNION ALL SELECT 'i_mr.mr_lcl',  ok(num((SELECT j FROM chart_i),'mr_lcl'), 0.0, 1e-9)
  UNION ALL SELECT 'xbar5.cl',     ok(num((SELECT j FROM chart_x5),'xbar_cl'), 29.98333333333333, 1e-6)
  UNION ALL SELECT 'xbar5.ucl',    ok(num((SELECT j FROM chart_x5),'xbar_ucl'), 30.262216666666664, 1e-6)
  UNION ALL SELECT 'xbar5.lcl',    ok(num((SELECT j FROM chart_x5),'xbar_lcl'), 29.704449999999998, 1e-6)
  UNION ALL SELECT 'xbar5.r_ucl',  ok(num((SELECT j FROM chart_x5),'r_ucl'),  1.021766666666665, 1e-6)
  UNION ALL SELECT 'xbar4.cl',     ok(num((SELECT j FROM chart_x4),'xbar_cl'), 40.8875, 1e-6)
  UNION ALL SELECT 'xbar4.ucl',    ok(num((SELECT j FROM chart_x4),'xbar_ucl'), 41.233775, 1e-6)
  UNION ALL SELECT 'err.bad_k',    (spc_chart('1.0,2.0,3.0', 'xbar_r', 2) IS NULL)
  UNION ALL SELECT 'err.nolimits', (spc_capability('1.0,2.0,3.0,4.0', '', '', '') IS NULL)
  UNION ALL SELECT 'err.badin',    (spc_capability('a,b,c', '1.0', '0.0', '') IS NULL)
  UNION ALL SELECT 'err.badchart', (spc_chart('1.0,2.0', 'bogus', 0) IS NULL);

SELECT
  COUNT(*) AS n_checks,
  COUNT(*) FILTER (WHERE NOT p) AS n_fail,
  string_agg(label, ', ' ORDER BY label) FILTER (WHERE NOT p) AS failing
FROM checks;
