#!/usr/bin/env python3
"""Authoritative oracle for duckdb_spc v0.1.0.

Computes the reference outputs for spc_capability and spc_chart on a fixed set
of inputs, and dumps JSON that the Rust + DuckDB e2e must match to ~1e-6.

Conventions (match JASP jaspQualityControl / Montgomery "Introduction to
Statistical Quality Control", Table VII constants):
  - overall sd  = sample sd (ddof=1)
  - within sd   = MRbar / d2(2)  (I-MR individuals; d2(2)=1.128)
  - Cp=(usl-lsl)/(6 sw);  Cpl=(mean-lsl)/(3 sw);  Cpu=(usl-mean)/(3 sw)
    Cpk=min of the present one-sided values;  Cpm (Bester/JMP) needs target
  - Pp/Ppl/Ppu/Ppk/Ppm/PT use overall sd (PT=(usl-lsl)/(6 s_o))
  - ppm: normal-approx nonconformance using overall sd
  - charts: I_MR and XBAR_R with published A2/D3/D4/d2 constants
"""
import json, math
import numpy as np
from scipy.stats import norm

# Standard SPC constants (Montgomery Table VII), subgroup size k = 2..10
A2 = {2:1.880,3:1.023,4:0.729,5:0.577,6:0.483,7:0.419,8:0.373,9:0.337,10:0.308}
D3 = {2:0.000,3:0.000,4:0.000,5:0.000,6:0.030,7:0.118,8:0.187,9:0.242,10:0.293}
D4 = {2:3.267,3:2.574,4:2.282,5:2.114,6:2.004,7:1.924,8:1.863,9:1.815,10:1.777}
d2 = {2:1.128,3:1.693,4:2.059,5:2.326,6:2.534,7:2.704,8:2.847,9:2.970,10:3.078}

def capability(x, usl, lsl, target):
    n = len(x)
    mean = float(np.mean(x))
    s_o = float(np.std(x, ddof=1))                 # overall
    # moving ranges (individuals)
    mr = [abs(x[i+1]-x[i]) for i in range(n-1)]
    mrbar = float(np.mean(mr))
    sw = mrbar / d2[2]                              # within (I-MR)

    def one(s, lo, hi):
        return (hi - s) / (3.0 * s if False else None)  # placeholder, replaced below
    out = {"n": n, "mean": mean, "sd_overall": s_o, "sd_within": sw, "mrbar": mrbar}

    # within
    cpl = (mean - lsl) / (3*sw) if lsl is not None else None
    cpu = (usl - mean) / (3*sw) if usl is not None else None
    if usl is not None and lsl is not None:
        cp = (usl - lsl) / (6*sw)
    else:
        cp = None
    cpk = None
    if cpu is not None and cpl is not None: cpk = min(cpu, cpl)
    elif cpu is not None: cpk = cpu
    elif cpl is not None: cpk = cpl
    cpm = None
    if target is not None and usl is not None and lsl is not None:
        cpm = (usl - lsl) / (6*sw*math.sqrt(1 + ((mean-target)/sw)**2))
    out.update({"cp":cp,"cpl":cpl,"cpu":cpu,"cpk":cpk,"cpm":cpm})

    # overall / performance
    ppl = (mean - lsl) / (3*s_o) if lsl is not None else None
    ppu = (usl - mean) / (3*s_o) if usl is not None else None
    pp = (usl - lsl) / (6*s_o) if (usl is not None and lsl is not None) else None
    ppk = None
    if ppu is not None and ppl is not None: ppk = min(ppu, ppl)
    elif ppu is not None: ppk = ppu
    elif ppl is not None: ppk = ppl
    ppm = None
    if target is not None and usl is not None and lsl is not None:
        ppm = (usl - lsl) / (6*s_o*math.sqrt(1 + ((mean-target)/s_o)**2))
    pt = (usl - lsl) / (6*s_o) if (usl is not None and lsl is not None) else None
    out.update({"pp":pp,"ppl":ppl,"ppu":ppu,"ppk":ppk,"ppm":ppm,"pt":pt})

    # nonconformance (overall, normal approx)
    below = norm.cdf((lsl-mean)/s_o) if lsl is not None else 0.0
    above = (1-norm.cdf((usl-mean)/s_o)) if usl is not None else 0.0
    frac = below + above
    out["pct_nc"] = frac*100.0
    out["ppm"] = frac*1e6
    return out

def chart_i_mr(x):
    n = len(x)
    mean = float(np.mean(x))
    mr = [abs(x[i+1]-x[i]) for i in range(n-1)]
    mrbar = float(np.mean(mr))
    sw = mrbar / d2[2]
    return {
        "cl": mean,
        "i_ucl": mean + 3*sw, "i_lcl": mean - 3*sw,
        "mr_cl": mrbar, "mr_ucl": D4[2]*mrbar, "mr_lcl": D3[2]*mrbar,
    }

def chart_xbar_r(x, k):
    assert len(x) % k == 0
    sg = np.array(x).reshape(-1, k)
    means = sg.mean(axis=1)
    ranges = (sg.max(axis=1) - sg.min(axis=1))
    xdouble = float(means.mean())
    rbar = float(ranges.mean())
    return {
        "xbar_cl": xdouble,
        "xbar_ucl": xdouble + A2[k]*rbar, "xbar_lcl": xdouble - A2[k]*rbar,
        "r_cl": rbar, "r_ucl": D4[k]*rbar, "r_lcl": D3[k]*rbar,
    }

def main():
    cases = []
    # capability datasets (deterministic, no RNG)
    cap_cases = [
        # (name, x, usl, lsl, target)
        ("cap_two_sided", [20.1,19.8,20.0,19.9,20.2,19.7,20.0,20.1,19.9,20.0,
                           20.3,19.6,20.0,20.1,19.8,20.0,19.9,20.2,19.8,20.0], 20.5, 19.5, 20.0),
        ("cap_two_sided_no_target", [10.5,9.8,10.1,9.9,10.2,10.0,10.3,9.7,10.0,10.1], 11.0, 9.0, None),
        ("cap_upper_only", [5.0,5.2,4.9,5.1,5.3,4.8,5.0,5.2,5.0,5.1,5.4,4.7], 5.5, None, None),
        ("cap_lower_only", [100.0,99.5,100.2,99.8,100.1,99.6,100.0,99.9,100.3,99.7], None, 99.0, 100.0),
        ("cap_off_center", [8.0,9.0,8.5,9.5,8.2,9.8,8.8,9.2,8.6,9.4,8.4,9.6], 9.0, 7.0, 8.0),
    ]
    for name, x, usl, lsl, tgt in cap_cases:
        cases.append({"kind":"capability","name":name,
                      "x":x,"usl":usl,"lsl":lsl,"target":tgt,
                      "ref":capability(x,usl,lsl,tgt)})

    # charts
    ird = [12.1,11.8,12.0,12.3,11.9,12.0,12.2,11.7,12.1,11.9,12.0,12.4,
           11.8,12.1,12.0,11.9,12.2,12.0,11.8,12.1,12.0,11.9,12.3,12.0]
    cases.append({"kind":"chart_i_mr","name":"i_mr","x":ird,
                  "ref":chart_i_mr(ird)})
    # XBAR_R, k=5, 6 subgroups of 5
    xbar = [30.1,29.8,30.0,29.9,30.2,   29.7,30.1,30.0,29.9,30.2,
            30.3,29.6,30.0,30.1,29.8,   30.0,29.9,30.2,29.8,30.0,
            29.9,30.1,29.7,30.2,30.0,   30.1,29.8,30.0,29.9,30.2]
    cases.append({"kind":"chart_xbar_r","name":"xbar_r_k5","x":xbar,"k":5,
                  "ref":chart_xbar_r(xbar,5)})
    xbar4 = [41.0,40.5,41.2,40.8,   40.9,41.1,40.7,41.0,   40.6,41.0,40.9,40.8,
             41.1,40.7,40.9,41.0]
    cases.append({"kind":"chart_xbar_r","name":"xbar_r_k4","x":xbar4,"k":4,
                  "ref":chart_xbar_r(xbar4,4)})

    # consistency check: A2 = 3/(d2*sqrt(k)) exact
    for k in range(2,11):
        calc = 3.0/(d2[k]*math.sqrt(k))
        assert abs(calc - A2[k]) < 0.002, f"A2 k={k} calc={calc} table={A2[k]}"
    print("A2 consistency check passed (3/(d2*sqrt(k)))")

    out = json.dumps(cases, indent=2)
    with open("/mnt/d/wsl2/duckdb_spc/test/oracle_ref.json","w") as f:
        f.write(out)
    print(f"wrote {len(cases)} cases -> oracle_ref.json")
    # print a couple for eyeballing
    for c in cases:
        if c["kind"]=="capability" and c["name"]=="cap_two_sided":
            r=c["ref"]
            print("cap_two_sided: mean=%.5f sw=%.5f so=%.5f Cp=%.5f Cpk=%.5f Cpm=%.5f Pp=%.5f Ppk=%.5f ppm=%.4f"%(
                r["mean"],r["sd_within"],r["sd_overall"],r["cp"],r["cpk"],r["cpm"],r["pp"],r["ppk"],r["ppm"]))
    for c in cases:
        if c["kind"]=="chart_xbar_r" and c["name"]=="xbar_r_k5":
            r=c["ref"]
            print("xbar_r_k5: Xdbar=%.5f Rbar=%.5f XUCL=%.5f XLCL=%.5f RUCL=%.5f"%(
                r["xbar_cl"],r["r_cl"],r["xbar_ucl"],r["xbar_lcl"],r["r_ucl"]))

if __name__=="__main__":
    main()
