window.BENCHMARK_DATA = {
  "lastUpdate": 1791666812371,
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
          "id": "ed6748e0fc2492d3f7710c3d0b5726697e3ea7a3",
          "message": "fix: xrdp healthcheck, bench noise threshold, nightly runner\n\nThe xrdp image ships no netcat, so its compose healthcheck could never pass (use bash /dev/tcp); the bench alert at 110% flapped on shared-runner CPU noise (subnet_calculate 59 vs 68 ns) — raise to 125%; nightly moves to ubuntu-24.04 whose flatpak-builder 1.4.x resolves sdk-extension refs that 22.04 1.2.2 cannot.",
          "timestamp": "2026-10-10T11:09:53+02:00",
          "tree_id": "cfa9fc2b59c056fbfd9ad4d055445f88d0b25bf0",
          "url": "https://github.com/leounakt23/LucYTerM/commit/ed6748e0fc2492d3f7710c3d0b5726697e3ea7a3"
        },
        "date": 1791623668686,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 73179,
            "range": "± 759",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 100056,
            "range": "± 241",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 213066,
            "range": "± 1264",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 395277,
            "range": "± 4527",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 13336,
            "range": "± 118",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 2942,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 54,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1464,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000404792,
            "range": "± 80798",
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
          "id": "1f43b545fadd479b5b2f5549832b7a671c2a15b5",
          "message": "fix: vnc healthcheck without nc; drop install-deps-from\n\nThe vnc image also ships no netcat (bash /dev/tcp probe). --install-deps-from=flathub rejects valid sdk-extension refs with \"Unknown extension in runtime\"; the workflow installs the rust-stable extension explicitly, so drop the flag and let flatpak-builder 1.4.x resolve against the user installation.",
          "timestamp": "2026-10-10T11:35:16+02:00",
          "tree_id": "8725d5de0177602b230a7e78951acd36fb9faa3b",
          "url": "https://github.com/leounakt23/LucYTerM/commit/1f43b545fadd479b5b2f5549832b7a671c2a15b5"
        },
        "date": 1791625180196,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 80426,
            "range": "± 669",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 103534,
            "range": "± 3490",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 206324,
            "range": "± 2734",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 435689,
            "range": "± 10134",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11706,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3328,
            "range": "± 109",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 60,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1920,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000582195,
            "range": "± 184313",
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
          "id": "e7fb3b1aa3f21e086aac1ff221b49bbd53dba07e",
          "message": "fix: full-ref sdk-extensions; bench alerts informational\n\nflatpak-builder 1.4.x concatenates the id//branch form into a malformed ref (id//branch/x86_64/branch) — use the full runtime ref. Bench baselines compare across runner hardware, so alert comments stay, fail-on-alert drops to false (two different benches flapped on identical code).",
          "timestamp": "2026-10-10T11:56:27+02:00",
          "tree_id": "1aaa90a9c173f2e7d39fab47090d6fcf9fe6ac40",
          "url": "https://github.com/leounakt23/LucYTerM/commit/e7fb3b1aa3f21e086aac1ff221b49bbd53dba07e"
        },
        "date": 1791626440970,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 80603,
            "range": "± 7142",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 103022,
            "range": "± 200",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 205665,
            "range": "± 2616",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 454457,
            "range": "± 2921",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11697,
            "range": "± 51",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3262,
            "range": "± 18",
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
            "value": 1912,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000538765,
            "range": "± 118068",
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
          "id": "c38a8b08c6c7f58b185c09d883e341545253b7cc",
          "message": "fix: bare extension id in flatpak sdk-extensions\n\nflatpak-builder appends arch/branch to each sdk-extensions entry itself; explicit //branch (1.2.x style) and full refs both produced doubled refs. The bare id is the correct 1.4.x form.",
          "timestamp": "2026-10-10T12:19:19+02:00",
          "tree_id": "497169f060450d6e3ce737c2c97bcfa01783aada",
          "url": "https://github.com/leounakt23/LucYTerM/commit/c38a8b08c6c7f58b185c09d883e341545253b7cc"
        },
        "date": 1791627840477,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 80867,
            "range": "± 1786",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 103364,
            "range": "± 590",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 207193,
            "range": "± 700",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 433231,
            "range": "± 7691",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11706,
            "range": "± 132",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3263,
            "range": "± 62",
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
            "value": 1915,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000401761,
            "range": "± 165996",
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
          "id": "1714e382c050e077ced0070b32a63883bdc04209",
          "message": "fix: flatpak expects the desktop file under the app id\n\nappstream compose accepts components by app-id; the desktop entry was installed as remote-app.desktop, so the composed catalog had zero components (filters-but-no-output). Install it as com.github.LucYTerM.remote-app.desktop and point the metainfo launchable at the new name. Reproduced and verified locally against appstream-compose 1.0.2.",
          "timestamp": "2026-10-10T13:40:40+02:00",
          "tree_id": "24a6c3891514049c124137cdaff361120df47986",
          "url": "https://github.com/leounakt23/LucYTerM/commit/1714e382c050e077ced0070b32a63883bdc04209"
        },
        "date": 1791632725808,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 68784,
            "range": "± 423",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 94024,
            "range": "± 1430",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 200069,
            "range": "± 2499",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 377528,
            "range": "± 7130",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 12970,
            "range": "± 195",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 2817,
            "range": "± 58",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 51,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1401,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000393094,
            "range": "± 33057",
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
          "id": "e6f0c03bd88eea069b22100fe451d393fe358b18",
          "message": "fix: install the SVG pixbuf loader for appstream compose\n\nappstream compose processes the scalable app icon through gdk-pixbuf; without librsvg2-common the SVG is unreadable and compose exits with file-read-error.",
          "timestamp": "2026-10-10T14:08:25+02:00",
          "tree_id": "195f9bcf18403a9ed4eeee27dec33391b695d9b9",
          "url": "https://github.com/leounakt23/LucYTerM/commit/e6f0c03bd88eea069b22100fe451d393fe358b18"
        },
        "date": 1791634372657,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 81048,
            "range": "± 1521",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 98276,
            "range": "± 941",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 201015,
            "range": "± 8832",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 431592,
            "range": "± 2089",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11950,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3264,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 66,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 2029,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000610600,
            "range": "± 52825",
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
          "id": "d05edc6fff241948ac259278f58eef271797f442",
          "message": "fix: make nightly artifact signing optional\n\nghaction-import-gpg aborted the whole build when GPG_PRIVATE_KEY is unset. Gate the import and signing steps on a job-level secret-presence flag; SHA256SUMS still publish unsigned until the key is configured.",
          "timestamp": "2026-10-10T14:32:42+02:00",
          "tree_id": "6105e1562e99ab9ce575fe44d328d50db64f01ba",
          "url": "https://github.com/leounakt23/LucYTerM/commit/d05edc6fff241948ac259278f58eef271797f442"
        },
        "date": 1791635848079,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 80104,
            "range": "± 808",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 102917,
            "range": "± 1883",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 206337,
            "range": "± 1973",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 456244,
            "range": "± 7292",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11698,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3257,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 61,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1905,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000528770,
            "range": "± 167125",
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
          "id": "cce605b158b9fbfd4a11755f42569c5ff540cc9b",
          "message": "chore: sync the fuzz lockfile with the unix nix dependency",
          "timestamp": "2026-10-10T23:05:11+02:00",
          "tree_id": "0a5135b4c3428a9538e0dd699d6f90dd34f79c63",
          "url": "https://github.com/leounakt23/LucYTerM/commit/cce605b158b9fbfd4a11755f42569c5ff540cc9b"
        },
        "date": 1791666812052,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 80398,
            "range": "± 487",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 103248,
            "range": "± 715",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 207017,
            "range": "± 2115",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 428317,
            "range": "± 1234",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11679,
            "range": "± 51",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3244,
            "range": "± 39",
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
            "value": 1908,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000406073,
            "range": "± 208841",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}