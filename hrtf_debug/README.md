# HRTF Debug References

Place local SOFA files under `hrtf_debug/sofa/` and run:

```powershell
python -m pip install h5py numpy
python hrtf_debug\analyze_sofa.py
python hrtf_debug\compare_elevation.py
```

The runtime does not load these SOFA files. They are local calibration
references for the parametric defaults in `crates/phrtf-dsp`.

For the RIEC files used here, the SOFA spherical azimuth convention is
`+90 = left` and `270 = right`. The plugin convention is `+90 = right`, so the
analysis script maps the right-side near ear from SOFA azimuth `270`.
