window.BENCHMARK_DATA = {
  "lastUpdate": 1791570457551,
  "repoUrl": "https://github.com/leounakt23/LucYTerM",
  "entries": {
    "Criterion benchmarks": [
      {
        "commit": {
          "author": {
            "email": "ms2k25@pm.me",
            "name": "leounakt23",
            "username": "leounakt23"
          },
          "committer": {
            "email": "ms2k25@pm.me",
            "name": "leounakt23",
            "username": "leounakt23"
          },
          "distinct": true,
          "id": "0198a877c3072bc7018047f344cee2681d5d53ca",
          "message": "fix: fuzzer-found u16 overflow in CSI cursor movement\n\nThe nightly fuzz campaign crashed vte_parser: with the cursor at the grid edge, ESC[65535C overflowed u16 addition (79 + 65535 > u16::MAX) in C/B/E handlers. Saturating add keeps the cursor clamped. Regression tests include the exact crash artifact. Also: bench wipes restored criterion baselines for determinism, flatpak gets the rust-stable SDK extension (cargo was missing, exit 127).",
          "timestamp": "2026-10-09T20:23:12+02:00",
          "tree_id": "1fc8ff86a76631bdb8f5d3c168352cbf562d55cb",
          "url": "https://github.com/leounakt23/LucYTerM/commit/0198a877c3072bc7018047f344cee2681d5d53ca"
        },
        "date": 1791570457246,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 80082,
            "range": "± 1183",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 102962,
            "range": "± 2676",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 206905,
            "range": "± 1343",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 464665,
            "range": "± 4770",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11706,
            "range": "± 22",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3247,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 59,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1931,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000501495,
            "range": "± 158693",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}