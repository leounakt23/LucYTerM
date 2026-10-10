window.BENCHMARK_DATA = {
  "lastUpdate": 1791622782495,
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
      },
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
          "id": "b41e033b09ffe6376c55597823bc985ae673019a",
          "message": "fix: let flatpak-builder resolve the rust SDK extension itself\n\nflatpak-builder 1.2.x does not see the user-installed rust-stable extension even though the install step completed; --install-deps-from=flathub makes it resolve sdk-extensions from the manifest.",
          "timestamp": "2026-10-10T09:35:55+02:00",
          "tree_id": "191d8d7afbec1643e138aecbb5c9f7cb3890f7b7",
          "url": "https://github.com/leounakt23/LucYTerM/commit/b41e033b09ffe6376c55597823bc985ae673019a"
        },
        "date": 1791618007964,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 80675,
            "range": "± 752",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 103499,
            "range": "± 595",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 208254,
            "range": "± 679",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 456495,
            "range": "± 7426",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11738,
            "range": "± 46",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3246,
            "range": "± 12",
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
            "value": 1907,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000564408,
            "range": "± 88693",
            "unit": "ns/iter"
          }
        ]
      },
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
          "id": "d95805fa4115f69cc6045873f321d98316e7ca3a",
          "message": "fix: let configure-pages enable the Actions build type\n\nPages was on the legacy/branch source; actions/deploy-pages requires the workflow build type and failed with BlobNotFound. configure-pages enablement switches it on first successful run (runner GITHUB_TOKEN has admin; the local gh token does not).",
          "timestamp": "2026-10-10T09:51:48+02:00",
          "tree_id": "b425b2ac46631fc989ca0d23821ca3984f4ff8f3",
          "url": "https://github.com/leounakt23/LucYTerM/commit/d95805fa4115f69cc6045873f321d98316e7ca3a"
        },
        "date": 1791619048315,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 80093,
            "range": "± 2866",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 103349,
            "range": "± 866",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 206551,
            "range": "± 797",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 455275,
            "range": "± 10655",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11795,
            "range": "± 151",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3262,
            "range": "± 83",
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
            "value": 1907,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000591495,
            "range": "± 210511",
            "unit": "ns/iter"
          }
        ]
      },
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
          "id": "c11d4aefb5adab5d18e1aa42413581988eaa44df",
          "message": "fix: move flatpak bundle to freedesktop 25.08\n\nThe 24.08 platform is end-of-life and its rust-stable extension no longer resolves (flatpak-builder: Unknown extension in runtime). Move runtime, sdk-extensions, and the CI install list to 25.08.",
          "timestamp": "2026-10-10T10:26:13+02:00",
          "tree_id": "7286f84c51fa5000ce49923048acdfb8c33cd3e2",
          "url": "https://github.com/leounakt23/LucYTerM/commit/c11d4aefb5adab5d18e1aa42413581988eaa44df"
        },
        "date": 1791621078298,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 79902,
            "range": "± 3650",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 103114,
            "range": "± 428",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 207229,
            "range": "± 3040",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 456277,
            "range": "± 7754",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11715,
            "range": "± 114",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3244,
            "range": "± 21",
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
            "value": 1919,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000382482,
            "range": "± 242235",
            "unit": "ns/iter"
          }
        ]
      },
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
          "id": "c7c2e957a5f1a5bb8c448971a7566cc4165da721",
          "message": "test: verify VNC and RDP against real servers\n\nVendored disposable servers: TigerVNC Xvnc (RFB 003.008, SecurityTypes None, BlacklistTimeout=0 so nc healthchecks cannot poison the listener) — vnc_integration passes 5/5 against it; xrdp+openbox — xfreerdp /auth-only passes with credentials, fails with a wrong password. VNC live suite is wired into the e2e workflow.",
          "timestamp": "2026-10-10T10:51:48+02:00",
          "tree_id": "c935c26c2d78384ed5e09ce6f388452c3cfbb2ad",
          "url": "https://github.com/leounakt23/LucYTerM/commit/c7c2e957a5f1a5bb8c448971a7566cc4165da721"
        },
        "date": 1791622782084,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 78557,
            "range": "± 328",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 99375,
            "range": "± 2383",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 202776,
            "range": "± 4576",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 431677,
            "range": "± 6271",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11925,
            "range": "± 29",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3260,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 68,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 2019,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000619384,
            "range": "± 109945",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}