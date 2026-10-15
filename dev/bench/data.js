window.BENCHMARK_DATA = {
  "lastUpdate": 1792102021180,
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
          "id": "871fb45fad8edfdd2221379192ade2c4b8e710b0",
          "message": "fix: let cold-cache CI legs finish within their limits",
          "timestamp": "2026-10-10T23:33:10+02:00",
          "tree_id": "fd07bc2f9663e36c7941cf739aecb4abfd79b10e",
          "url": "https://github.com/leounakt23/LucYTerM/commit/871fb45fad8edfdd2221379192ade2c4b8e710b0"
        },
        "date": 1791668280035,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 69379,
            "range": "± 426",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 91230,
            "range": "± 233",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 203503,
            "range": "± 1709",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 400848,
            "range": "± 9612",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11935,
            "range": "± 122",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 2695,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 58,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1437,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000416338,
            "range": "± 65875",
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
          "id": "5a5a02faee32dd5e2fef0574775286ec158c4460",
          "message": "security: record the full advisory inventory and the pinned-stack migration path",
          "timestamp": "2026-10-11T00:10:09+02:00",
          "tree_id": "f9826743619cf08fea4c78cd6f51f0ade14f299e",
          "url": "https://github.com/leounakt23/LucYTerM/commit/5a5a02faee32dd5e2fef0574775286ec158c4460"
        },
        "date": 1791670456738,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 63311,
            "range": "± 905",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 76569,
            "range": "± 948",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 155738,
            "range": "± 436",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 333191,
            "range": "± 684",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 9310,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 2619,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 50,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1583,
            "range": "± 43",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000419040,
            "range": "± 42810",
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
          "id": "566f2c09689859ec648b1ad37834caf5117fe017",
          "message": "fix: point the private reporting channel at the real repository",
          "timestamp": "2026-10-11T00:41:25+02:00",
          "tree_id": "6475ed55b0a7776102d1df7572a7533a16d60f5d",
          "url": "https://github.com/leounakt23/LucYTerM/commit/566f2c09689859ec648b1ad37834caf5117fe017"
        },
        "date": 1791672345300,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 81447,
            "range": "± 217",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 104988,
            "range": "± 3366",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 207803,
            "range": "± 5368",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 439880,
            "range": "± 11607",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11781,
            "range": "± 66",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3269,
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
            "value": 1909,
            "range": "± 108",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000634301,
            "range": "± 217605",
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
          "id": "8a555591fb0f17f9cb6c56aadd427fd2bfe2f218",
          "message": "feat: session edit and delete in the GUI with shared validation",
          "timestamp": "2026-10-11T01:24:38+02:00",
          "tree_id": "91bcb724fa05677f0be3658c75cc4e7a1a97c5e8",
          "url": "https://github.com/leounakt23/LucYTerM/commit/8a555591fb0f17f9cb6c56aadd427fd2bfe2f218"
        },
        "date": 1791674920668,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 46164,
            "range": "± 1726",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 64955,
            "range": "± 3654",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 137578,
            "range": "± 3396",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 241031,
            "range": "± 3245",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 8286,
            "range": "± 177",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 1627,
            "range": "± 54",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 50,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1020,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000324748,
            "range": "± 114678",
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
          "id": "2c1626a1f539a63e92895089ea840d043ec8d7dd",
          "message": "test: fixture-driven terminal rendering checks for VT100 and colors",
          "timestamp": "2026-10-11T02:02:45+02:00",
          "tree_id": "86d0676c65ff4c80ff7f9f29c44a04ab000eb342",
          "url": "https://github.com/leounakt23/LucYTerM/commit/2c1626a1f539a63e92895089ea840d043ec8d7dd"
        },
        "date": 1791677198512,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 49135,
            "range": "± 363",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 68779,
            "range": "± 550",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 142937,
            "range": "± 638",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 261209,
            "range": "± 2501",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 8999,
            "range": "± 62",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 1733,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 54,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1107,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000415790,
            "range": "± 143581",
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
          "id": "ecd152e7e873212411685254808ca44a1c9692db",
          "message": "test: SSH password, key, and agent logins against disposable OpenSSH",
          "timestamp": "2026-10-11T02:16:00+02:00",
          "tree_id": "4bedd4180793c479b2cfff8e9e763801623bf81a",
          "url": "https://github.com/leounakt23/LucYTerM/commit/ecd152e7e873212411685254808ca44a1c9692db"
        },
        "date": 1791678040486,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 80776,
            "range": "± 1387",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 102837,
            "range": "± 2067",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 207381,
            "range": "± 1410",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 434739,
            "range": "± 8261",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11721,
            "range": "± 48",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3235,
            "range": "± 15",
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
            "value": 1917,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000576673,
            "range": "± 157215",
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
          "id": "f333366a4f9380fb934db3c354c990e7d116c75d",
          "message": "test: macro record-to-playback round trip; tick verified checklist items",
          "timestamp": "2026-10-11T08:26:01+02:00",
          "tree_id": "c377cd5ed5319fc4c6b164a02f2cf583340d0a46",
          "url": "https://github.com/leounakt23/LucYTerM/commit/f333366a4f9380fb934db3c354c990e7d116c75d"
        },
        "date": 1791700225659,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 49097,
            "range": "± 3172",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 66620,
            "range": "± 2117",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 145847,
            "range": "± 2193",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 300608,
            "range": "± 4315",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 9326,
            "range": "± 52",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 1969,
            "range": "± 28",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 38,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 837,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000328003,
            "range": "± 32658",
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
          "id": "4c2391f4a72da20e32a862e91518856019e26d57",
          "message": "docs: Xephyr click-through proves session CRUD and validation live",
          "timestamp": "2026-10-11T09:51:25+02:00",
          "tree_id": "187b28c39b2d78af838b8ff95fdcd34ccbe38ac5",
          "url": "https://github.com/leounakt23/LucYTerM/commit/4c2391f4a72da20e32a862e91518856019e26d57"
        },
        "date": 1791705324976,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 45589,
            "range": "± 518",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 63234,
            "range": "± 644",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 130750,
            "range": "± 1204",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 240722,
            "range": "± 4410",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 8240,
            "range": "± 77",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 1610,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 49,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1017,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000237193,
            "range": "± 108748",
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
          "id": "89de664f380adb67b051428f6a040ed95237e727",
          "message": "feat: terminal keyboard input, clipboard copy/paste, mouse reporting",
          "timestamp": "2026-10-13T12:55:57+02:00",
          "tree_id": "f016815ae790fdd01f90136218bb74048585b3de",
          "url": "https://github.com/leounakt23/LucYTerM/commit/89de664f380adb67b051428f6a040ed95237e727"
        },
        "date": 1791889350618,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 48235,
            "range": "± 1634",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 68305,
            "range": "± 1352",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 139961,
            "range": "± 3870",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 255669,
            "range": "± 19090",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 8930,
            "range": "± 124",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 1719,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 51,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1059,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000365376,
            "range": "± 169021",
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
          "id": "ebd270506fdcb2ca68c295771b6b8d1907aa7235",
          "message": "test: merge racing clipboard tests into one deterministic test",
          "timestamp": "2026-10-13T18:15:56+02:00",
          "tree_id": "fde1e8e15112009b49963cf1f336af8a69a3373a",
          "url": "https://github.com/leounakt23/LucYTerM/commit/ebd270506fdcb2ca68c295771b6b8d1907aa7235"
        },
        "date": 1791908440966,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 63756,
            "range": "± 240",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 80086,
            "range": "± 418",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 156580,
            "range": "± 1141",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 335782,
            "range": "± 946",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 9693,
            "range": "± 27",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 2486,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 50,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1591,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000436829,
            "range": "± 72891",
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
          "id": "59996279aef9a9f026ac0c147fb482a1bbdee398",
          "message": "feat: per-row disconnect button with live Xephyr roundtrip proof",
          "timestamp": "2026-10-14T00:28:00+02:00",
          "tree_id": "44fc22c2d760f21472607565dfd07139d0db1c98",
          "url": "https://github.com/leounakt23/LucYTerM/commit/59996279aef9a9f026ac0c147fb482a1bbdee398"
        },
        "date": 1791930985165,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 80918,
            "range": "± 1480",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 102586,
            "range": "± 312",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 208208,
            "range": "± 933",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 428214,
            "range": "± 1818",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11710,
            "range": "± 22",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3268,
            "range": "± 14",
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
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000473439,
            "range": "± 188319",
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
          "id": "ce9ef2535c9356d297c964f85c8c283fd2b2777f",
          "message": "security: redact host/user/secrets from Debug and logs, warn on --password",
          "timestamp": "2026-10-14T03:39:05+02:00",
          "tree_id": "da10b468eec9a4ef9e057269b87cfe1f2dbf7a48",
          "url": "https://github.com/leounakt23/LucYTerM/commit/ce9ef2535c9356d297c964f85c8c283fd2b2777f"
        },
        "date": 1791942227173,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 69241,
            "range": "± 826",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 91189,
            "range": "± 570",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 201161,
            "range": "± 989",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 396477,
            "range": "± 2206",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11837,
            "range": "± 73",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 2693,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 58,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1440,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000348614,
            "range": "± 76687",
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
          "id": "468cf3600044b33930b529a574a80b51a93c53f8",
          "message": "feat: third-party session import with published matrix and CLI",
          "timestamp": "2026-10-14T03:49:38+02:00",
          "tree_id": "cb74908d065e81fccc7bdd576da104292e572afa",
          "url": "https://github.com/leounakt23/LucYTerM/commit/468cf3600044b33930b529a574a80b51a93c53f8"
        },
        "date": 1791942849329,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 79923,
            "range": "± 258",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 102432,
            "range": "± 952",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 207312,
            "range": "± 1060",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 456394,
            "range": "± 11551",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11785,
            "range": "± 56",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3291,
            "range": "± 15",
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
            "value": 1906,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000594099,
            "range": "± 102182",
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
          "id": "e18beedf671b62ba84aff9ef89a5f367346df488",
          "message": "docs: record distribution recipe hardening, keep install boxes unchecked",
          "timestamp": "2026-10-14T06:16:47+02:00",
          "tree_id": "e333a672f2d606782e1611ca3a82674b3a9e5ee3",
          "url": "https://github.com/leounakt23/LucYTerM/commit/e18beedf671b62ba84aff9ef89a5f367346df488"
        },
        "date": 1791951736207,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 62937,
            "range": "± 2118",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 76246,
            "range": "± 522",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 154656,
            "range": "± 551",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 334474,
            "range": "± 2704",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 9279,
            "range": "± 50",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 2596,
            "range": "± 83",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 50,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1590,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000464833,
            "range": "± 41725",
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
          "id": "86cb298ed4aafba2469bf40ccbb42d2315ce8553",
          "message": "security: dump-hardening tests, audit triage, close log/dump/audit checklist items",
          "timestamp": "2026-10-14T06:22:55+02:00",
          "tree_id": "a38e98e2af12c5d277a5f04f99746053470b1e9f",
          "url": "https://github.com/leounakt23/LucYTerM/commit/86cb298ed4aafba2469bf40ccbb42d2315ce8553"
        },
        "date": 1791952037197,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 80522,
            "range": "± 1388",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 102793,
            "range": "± 293",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 207939,
            "range": "± 1335",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 458006,
            "range": "± 2385",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11720,
            "range": "± 79",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3252,
            "range": "± 13",
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
            "value": 1910,
            "range": "± 43",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000444897,
            "range": "± 158490",
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
          "id": "dd2221556465ff4fdc4488d1ece1152f11ee2b5d",
          "message": "perf: frame/ssh/sftp/soak harness with measured results",
          "timestamp": "2026-10-14T06:52:06+02:00",
          "tree_id": "a592450b474bbf18ae48ff9b970647f152c394a7",
          "url": "https://github.com/leounakt23/LucYTerM/commit/dd2221556465ff4fdc4488d1ece1152f11ee2b5d"
        },
        "date": 1791954077079,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 78335,
            "range": "± 1299",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 98759,
            "range": "± 1012",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 201016,
            "range": "± 498",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 423213,
            "range": "± 14686",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11971,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3349,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 67,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1928,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000629504,
            "range": "± 46874",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_render_80x24",
            "value": 475258,
            "range": "± 1173",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_render_160x50",
            "value": 1208676,
            "range": "± 7669",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_flood_240frames",
            "value": 28987225,
            "range": "± 64316",
            "unit": "ns/iter"
          },
          {
            "name": "session_soak_10x80x24_rss",
            "value": 5951711,
            "range": "± 28672",
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
          "id": "63b3fe95000ce23c466b7d7a89f30a73304217b2",
          "message": "test: 8h ten-session soak harness with full-run RSS evidence",
          "timestamp": "2026-10-15T21:35:18+02:00",
          "tree_id": "d24a48dedb901d2415efbe90701d7d0bf0a48b89",
          "url": "https://github.com/leounakt23/LucYTerM/commit/63b3fe95000ce23c466b7d7a89f30a73304217b2"
        },
        "date": 1792093223688,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 81111,
            "range": "± 3356",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 103090,
            "range": "± 486",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 206884,
            "range": "± 723",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 423262,
            "range": "± 12247",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11775,
            "range": "± 28",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3313,
            "range": "± 43",
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
            "value": 1897,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000478463,
            "range": "± 181087",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_render_80x24",
            "value": 504973,
            "range": "± 1951",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_render_160x50",
            "value": 1184006,
            "range": "± 20426",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_flood_240frames",
            "value": 26070182,
            "range": "± 702281",
            "unit": "ns/iter"
          },
          {
            "name": "session_soak_10x80x24_rss",
            "value": 8663416,
            "range": "± 102949",
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
          "id": "48a3d94833a99a8aa8b9c24028cf815bdaa18d15",
          "message": "build: close all six distro install boxes with container builds\n\nRPM (fedora:40 dnf install), AUR (makepkg !lto + pacman), AppImage\n(extract-and-run), Flatpak (flatpak-builder + Xvfb run), Snap\n(snapcraft --destructive-mode + snap install), Nix (flake build +\nclean-store smoke) all build, install, and launch remote-app 0.1.0.\nRecipes: AUR !lto + real sha256 for pinned commit; RPM _builddir\nstaging + librsvg2-tools; snap libc6-dev build package; flatpak\nrename-icon, appstream host note, cargo net retries.",
          "timestamp": "2026-10-15T22:38:07+02:00",
          "tree_id": "0fb397a4b6f2eeb85cd481bfc47b0ef2fcc18cd5",
          "url": "https://github.com/leounakt23/LucYTerM/commit/48a3d94833a99a8aa8b9c24028cf815bdaa18d15"
        },
        "date": 1792096999193,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 61198,
            "range": "± 291",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 76915,
            "range": "± 2669",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 156045,
            "range": "± 350",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 332345,
            "range": "± 423",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 9269,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 2565,
            "range": "± 22",
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
            "value": 1510,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000448400,
            "range": "± 43057",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_render_80x24",
            "value": 369017,
            "range": "± 2043",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_render_160x50",
            "value": 939248,
            "range": "± 5497",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_flood_240frames",
            "value": 22477645,
            "range": "± 294469",
            "unit": "ns/iter"
          },
          {
            "name": "session_soak_10x80x24_rss",
            "value": 5556180,
            "range": "± 61283",
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
          "id": "1e728198dc4e14299e9ddfca0e1a8bdc6a0c9f6d",
          "message": "docs: close CI matrix, actionlint, and coverage checklist items\n\nAlso refresh the final_report performance table rows that still said\nUNMEASURED: 60 FPS (headless CPU path), SSH connect (disposable\ncontainer N=10), and 10 sessions (8 h soak, 14.8 MiB peak).",
          "timestamp": "2026-10-15T22:43:43+02:00",
          "tree_id": "12c79f7b55b0e70b74a5b449ac933bbf91980ec6",
          "url": "https://github.com/leounakt23/LucYTerM/commit/1e728198dc4e14299e9ddfca0e1a8bdc6a0c9f6d"
        },
        "date": 1792097342347,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 72356,
            "range": "± 831",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 97161,
            "range": "± 233",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 205372,
            "range": "± 728",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 380161,
            "range": "± 1752",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 13019,
            "range": "± 145",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 2929,
            "range": "± 37",
            "unit": "ns/iter"
          },
          {
            "name": "subnet_calculate",
            "value": 53,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "tool_export_json",
            "value": 1708,
            "range": "± 43",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000403986,
            "range": "± 48518",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_render_80x24",
            "value": 805016,
            "range": "± 14463",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_render_160x50",
            "value": 1403615,
            "range": "± 23250",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_flood_240frames",
            "value": 22132511,
            "range": "± 126115",
            "unit": "ns/iter"
          },
          {
            "name": "session_soak_10x80x24_rss",
            "value": 12848893,
            "range": "± 359130",
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
          "id": "0c8b0359580d1b3ae79bcf9334e4da5cf177a564",
          "message": "docs: close the last checklist item with Wayland and AUR 1.0.0 evidence\n\nQuality item: expanded seven-leg CI matrix, X11 evidence, and the\nheadless-weston Wayland smoke (full UI rendered at 1280x800). AUR row\nre-verified at 1.0.0 end-to-end; distro section notes the 1.0.0 recipe\nbumps and release-workflow artifacts. All checklist boxes are now\nchecked.",
          "timestamp": "2026-10-15T23:36:19+02:00",
          "tree_id": "2f5354d6e5c668ac11977b0778cad6cc37d7b35f",
          "url": "https://github.com/leounakt23/LucYTerM/commit/0c8b0359580d1b3ae79bcf9334e4da5cf177a564"
        },
        "date": 1792102020761,
        "tool": "cargo",
        "benches": [
          {
            "name": "terminal/scroll_region_up/24",
            "value": 79915,
            "range": "± 1563",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/100",
            "value": 101550,
            "range": "± 865",
            "unit": "ns/iter"
          },
          {
            "name": "terminal/scroll_region_up/500",
            "value": 205216,
            "range": "± 1587",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_search_160x500",
            "value": 410982,
            "range": "± 3642",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_vte_output",
            "value": 11722,
            "range": "± 40",
            "unit": "ns/iter"
          },
          {
            "name": "config_ron_serialization",
            "value": 3264,
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
            "value": 1731,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "transfer_loopback/bandwidth_sender_1s",
            "value": 1000494854,
            "range": "± 212887",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_render_80x24",
            "value": 522745,
            "range": "± 8677",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_render_160x50",
            "value": 1219519,
            "range": "± 60835",
            "unit": "ns/iter"
          },
          {
            "name": "terminal_frame/headless_flood_240frames",
            "value": 26462335,
            "range": "± 92055",
            "unit": "ns/iter"
          },
          {
            "name": "session_soak_10x80x24_rss",
            "value": 9419434,
            "range": "± 161824",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}