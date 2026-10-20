//! duckdb_spc — Statistical Process Control (JASP jaspQualityControl subset).
//!
//! v0.1.0: two scalar functions returning JSON (all closed-form, no
//! convergence, verified against the scipy oracle in `test/oracle.py`):
//!
//!   spc_capability(x VARCHAR, usl DOUBLE, lsl DOUBLE, target DOUBLE) → VARCHAR(JSON)
//!     Process capability (within σ via I-MR) + performance (overall sd):
//!     Cp/Cpl/Cpu/Cpk/Cpm, Pp/Ppl/Ppu/Ppk/Ppm/PT, plus normal-approx
//!     nonconformance (pct_nc, ppm). usl/lsl/target = NULL to omit a side.
//!
//!   spc_chart(x VARCHAR, chart VARCHAR, k BIGINT) → VARCHAR(JSON)
//!     Control-chart limits. chart='i_mr' (individuals) or 'xbar_r'
//!     (subgroup size k, values consecutive k-groups).
//!
//!   x: flat measurements as a string ("20.1,19.8\n19.9 ..." — commas or
//!      whitespace separated).
//!
//! Math in `src/math.rs`. Conventions: JASP jaspQualityControl / Montgomery
//! Table-VII constants (A2/D3/D4/d2, k=2..10).

pub mod math;

use quack_rs::{
    entry_point::init_extension_v2,
    prelude::*,
    vector::{VectorReader, VectorWriter},
};
use libduckdb_sys::{
    duckdb_data_chunk, duckdb_data_chunk_get_size, duckdb_extension_access,
    duckdb_extension_info, duckdb_vector,
};
use std::collections::BTreeMap;

/// Serialize a metric map to a compact JSON object string. Keys in sorted
/// (BTree) order. None → JSON null.
fn to_json(map: &BTreeMap<String, Option<f64>>) -> String {
    let mut s = String::from("{");
    for (k, v) in map {
        s.push('"');
        s.push_str(k);
        s.push_str("\":");
        match v {
            None => s.push_str("null"),
            Some(x) => {
                if x == &f64::INFINITY {
                    s.push_str("1.7976931348623157e308");
                } else if x == &f64::NEG_INFINITY {
                    s.push_str("-1.7976931348623157e308");
                } else if x.is_nan() {
                    s.push_str("null");
                } else {
                    s.push_str(&format!("{:.17e}", x));
                }
            }
        }
        s.push(',');
    }
    if s.ends_with(',') {
        s.pop();
    }
    s.push('}');
    s
}

/// Serialize an all-numeric metric map to compact JSON.
fn to_json_num(map: &BTreeMap<String, f64>) -> String {
    let mut s = String::from("{");
    for (k, v) in map {
        s.push('"');
        s.push_str(k);
        s.push_str("\":");
        if v.is_nan() {
            s.push_str("null");
        } else {
            s.push_str(&format!("{:.17e}", v));
        }
        s.push(',');
    }
    if s.ends_with(',') {
        s.pop();
    }
    s.push('}');
    s
}

unsafe extern "C" fn spc_capability_fn(
    _info: libduckdb_sys::duckdb_function_info,
    input: duckdb_data_chunk,
    output: duckdb_vector,
) {
    let n = usize::try_from(unsafe { duckdb_data_chunk_get_size(input) }).unwrap_or(0);
    let rx = unsafe { VectorReader::new(input, 0) }; // x VARCHAR
    let ru = unsafe { VectorReader::new(input, 1) }; // usl DOUBLE
    let rl = unsafe { VectorReader::new(input, 2) }; // lsl DOUBLE
    let rt = unsafe { VectorReader::new(input, 3) }; // target DOUBLE
    let mut out = unsafe { VectorWriter::new(output) };
    for i in 0..n {
        if !unsafe { rx.is_valid(i) } {
            unsafe { out.set_null(i) };
            continue;
        }
        let s = unsafe { rx.read_str(i) };
        let x = match math::parse_numbers(s) {
            Ok(v) => v,
            Err(_) => {
                unsafe { out.set_null(i) };
                continue;
            }
        };
        if x.len() < 2 {
            unsafe { out.set_null(i) };
            continue;
        }
        // spec limits are VARCHAR: empty / NA / none / NULL / - means absent.
        // (DuckDB auto-propagates NULL inputs to the scalar output, so the
        // "absent" sentinel must be a string token, not SQL NULL.)
        let usl = if unsafe { ru.is_valid(i) } {
            math::parse_limit(unsafe { ru.read_str(i) })
        } else {
            None
        };
        let lsl = if unsafe { rl.is_valid(i) } {
            math::parse_limit(unsafe { rl.read_str(i) })
        } else {
            None
        };
        let target = if unsafe { rt.is_valid(i) } {
            math::parse_limit(unsafe { rt.read_str(i) })
        } else {
            None
        };
        if usl.is_none() && lsl.is_none() {
            // nothing to compare against — no capability to report
            unsafe { out.set_null(i) };
            continue;
        }
        let map = math::capability(&x, usl, lsl, target);
        let json = to_json(&map);
        unsafe { out.write_str(i, &json) };
    }
}

unsafe extern "C" fn spc_chart_fn(
    _info: libduckdb_sys::duckdb_function_info,
    input: duckdb_data_chunk,
    output: duckdb_vector,
) {
    let n = usize::try_from(unsafe { duckdb_data_chunk_get_size(input) }).unwrap_or(0);
    let rx = unsafe { VectorReader::new(input, 0) }; // x VARCHAR
    let rc = unsafe { VectorReader::new(input, 1) }; // chart VARCHAR
    let rk = unsafe { VectorReader::new(input, 2) }; // k BIGINT
    let mut out = unsafe { VectorWriter::new(output) };
    for i in 0..n {
        if !unsafe { rx.is_valid(i) } || !unsafe { rc.is_valid(i) } {
            unsafe { out.set_null(i) };
            continue;
        }
        let s = unsafe { rx.read_str(i) };
        let chart = unsafe { rc.read_str(i) };
        let x = match math::parse_numbers(s) {
            Ok(v) => v,
            Err(_) => {
                unsafe { out.set_null(i) };
                continue;
            }
        };
        let k = if unsafe { rk.is_valid(i) } {
            (unsafe { rk.read_i64(i) }) as usize
        } else {
            0
        };
        let map = match chart {
            "i_mr" => {
                if x.len() < 2 {
                    unsafe { out.set_null(i) };
                    continue;
                }
                to_json_num(&math::chart_i_mr(&x))
            }
            "xbar_r" => match math::chart_xbar_r(&x, k) {
                Some(m) => to_json_num(&m),
                None => {
                    unsafe { out.set_null(i) };
                    continue;
                }
            },
            _ => {
                unsafe { out.set_null(i) };
                continue;
            }
        };
        unsafe { out.write_str(i, &map) };
    }
}

fn register_all(reg: &impl Registrar) -> Result<(), ExtensionError> {
    unsafe {
        reg.register_scalar(
            ScalarFunctionBuilder::new("spc_capability")
                .param(TypeId::Varchar)
                .param(TypeId::Varchar)
                .param(TypeId::Varchar)
                .param(TypeId::Varchar)
                .returns(TypeId::Varchar)
                .function(spc_capability_fn),
        )?;
        reg.register_scalar(
            ScalarFunctionBuilder::new("spc_chart")
                .param(TypeId::Varchar)
                .param(TypeId::Varchar)
                .param(TypeId::BigInt)
                .returns(TypeId::Varchar)
                .function(spc_chart_fn),
        )?;
    }
    Ok(())
}

#[no_mangle]
pub unsafe extern "C" fn spc_ext_init_c_api(
    info: duckdb_extension_info,
    access: *const duckdb_extension_access,
) -> bool {
    unsafe {
        init_extension_v2(info, access, quack_rs::DUCKDB_API_VERSION, |con| {
            register_all(con)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 * (1.0 + a.abs().max(b.abs()))
    }

    #[test]
    fn parse_numbers_basic() {
        let v = math::parse_numbers("20.1,19.8\n5.2 19.9").unwrap();
        assert_eq!(v, vec![20.1, 19.8, 5.2, 19.9]);
        assert!(math::parse_numbers("a b").is_err());
        assert!(math::parse_numbers("1, 2").unwrap() == vec![1.0, 2.0]);
    }

    #[test]
    fn parse_limit_sentinels() {
        assert_eq!(math::parse_limit(""), None);
        assert_eq!(math::parse_limit("NA"), None);
        assert_eq!(math::parse_limit("none"), None);
        assert_eq!(math::parse_limit("NULL"), None);
        assert_eq!(math::parse_limit("-"), None);
        assert_eq!(math::parse_limit(" 5.5 "), Some(5.5));
        assert_eq!(math::parse_limit("-3.2"), Some(-3.2));
        assert!(math::parse_limit("abc").is_none());
    }

    fn get<'a>(m: &'a BTreeMap<String, Option<f64>>, k: &str) -> f64 {
        m.get(k).unwrap().unwrap()
    }

    #[test]
    fn norm_cdf_sanity() {
        assert!(approx(math::norm_cdf(0.0), 0.5));
        assert!(approx(math::norm_cdf(1.959964), 0.975));
        assert!(math::norm_cdf(-3.0) < 0.002);
    }

    #[test]
    fn capability_two_sided_oracle() {
        let x: Vec<f64> = vec![
            20.1, 19.8, 20.0, 19.9, 20.2, 19.7, 20.0, 20.1, 19.9, 20.0, 20.3, 19.6, 20.0,
            20.1, 19.8, 20.0, 19.9, 20.2, 19.8, 20.0,
        ];
        let r = math::capability(&x, Some(20.5), Some(19.5), Some(20.0));
        assert!(approx(get(&r, "mean"), 19.97));
        assert!(approx(get(&r, "sd_within"), 0.23796192609182573));
        assert!(approx(get(&r, "cp"), 0.7003921568627439));
        assert!(approx(get(&r, "cpk"), 0.6583686274509777));
        assert!(approx(get(&r, "cpm"), 0.69489169406063));
        assert!(approx(get(&r, "pp"), 0.9522786723020039));
        assert!(approx(get(&r, "ppm"), 4851.760316213353));
        assert!(approx(get(&r, "pct_nc"), 0.48517603162133527));
    }

    #[test]
    fn capability_upper_only_oracle() {
        let x: Vec<f64> = vec![5.0, 5.2, 4.9, 5.1, 5.3, 4.8, 5.0, 5.2, 5.0, 5.1, 5.4, 4.7];
        let r = math::capability(&x, Some(5.5), None, None);
        assert!(r.get("cp").unwrap().is_none());
        assert!(approx(get(&r, "cpu"), 0.5892688172043006));
        assert!(approx(get(&r, "cpk"), 0.5892688172043006));
        assert!(approx(get(&r, "ppm"), 14419.396813245112));
    }

    #[test]
    fn capability_off_center_oracle() {
        let x: Vec<f64> = vec![
            8.0, 9.0, 8.5, 9.5, 8.2, 9.8, 8.8, 9.2, 8.6, 9.4, 8.4, 9.6,
        ];
        let r = math::capability(&x, Some(9.0), Some(7.0), Some(8.0));
        assert!(approx(get(&r, "cpk"), 0.033141025641025876));
        assert!(approx(get(&r, "ppk"), 0.04715760341752041));
        assert!(approx(get(&r, "ppm"), 444317.4316642531));
    }

    #[test]
    fn a2_consistency() {
        // A2(k) == 3 / (d2(k) * sqrt(k))
        for k in 2..=10 {
            let calc = 3.0 / (math::D2[k] * (k as f64).sqrt());
            assert!((calc - math::A2[k]).abs() < 0.002, "k={} {} {}", k, calc, math::A2[k]);
        }
    }

    #[test]
    fn chart_i_mr_oracle() {
        let x: Vec<f64> = vec![
            12.1, 11.8, 12.0, 12.3, 11.9, 12.0, 12.2, 11.7, 12.1, 11.9, 12.0, 12.4, 11.8,
            12.1, 12.0, 11.9, 12.2, 12.0, 11.8, 12.1, 12.0, 11.9, 12.3, 12.0,
        ];
        let r = math::chart_i_mr(&x);
        assert!(approx(r["cl"], 12.020833333333334));
        assert!(approx(r["i_ucl"], 12.726198735738514));
        assert!(approx(r["mr_ucl"], 0.8664652173913033));
        assert!(approx(r["mr_lcl"], 0.0));
    }

    #[test]
    fn chart_xbar_r_oracle() {
        let x: Vec<f64> = vec![
            30.1, 29.8, 30.0, 29.9, 30.2, 29.7, 30.1, 30.0, 29.9, 30.2, 30.3, 29.6, 30.0,
            30.1, 29.8, 30.0, 29.9, 30.2, 29.8, 30.0, 29.9, 30.1, 29.7, 30.2, 30.0, 30.1,
            29.8, 30.0, 29.9, 30.2,
        ];
        let r = math::chart_xbar_r(&x, 5).unwrap();
        assert!(approx(r["xbar_cl"], 29.98333333333333));
        assert!(approx(r["xbar_ucl"], 30.262216666666664));
        assert!(approx(r["xbar_lcl"], 29.704449999999998));
        assert!(approx(r["r_ucl"], 1.021766666666665));
    }

    #[test]
    fn chart_xbar_r_rejects_bad_k() {
        let x: Vec<f64> = vec![1.0, 2.0, 3.0];
        assert!(math::chart_xbar_r(&x, 2).is_none()); // 3 not divisible by 2
        assert!(math::chart_xbar_r(&x, 11).is_none()); // k out of range
    }
}
