#!/usr/bin/env python3
"""WorldSimulator canonical history seed builder.

Builds the immutable canonical timeline SQLite database from:

  1. Natural Earth admin-0 country polygons (bundled in data/raw) for real
     modern borders and geometry for every era (territories in historical
     eras are modern shapes assigned to their historical owner).
  2. Curated era ownership maps for the major states of each era
     (3200 BCE -> 2020 CE).
  3. Real narrative milestones including the Paleolithic: first controlled
     fire by Homo erectus, migrations out of Africa, Neanderthals,
     Denisovans, first cave art, the Agricultural Revolution, and the
     invention of writing in Sumer.

Output: data/out/worldsim.db (schema matches the Rust engine).

Usage:  python3 data/build_seed.py
"""

import json
import os
import sqlite3
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
RAW = os.path.join(HERE, "raw", "ne_110m_admin_0_countries.geojson")
OUT = os.path.join(HERE, "out", "worldsim.db")

PRESENT_YEAR = 2026
DAYS_ZERO_CE = 719468


def days_from_ce(y, m, d):
    """Match the Rust engine's proleptic Gregorian day count (astronomical year)."""
    y -= 1 if m <= 2 else 0
    era = (y if y >= 0 else y - 399) // 400
    yoe = y - era * 400
    doy = (153 * (m + (-3 if m > 2 else 9)) + 2) // 5 + d - 1
    doe = yoe * 365 + yoe // 4 - yoe // 100 + doy
    return era * 146097 + doe - DAYS_ZERO_CE


def load_ne():
    with open(RAW) as f:
        data = json.load(f)
    countries = []
    for feat in data["features"]:
        p = feat["properties"]
        iso = p.get("ADM0_A3") or p.get("ADMIN") or "?"
        if iso == "ATA":
            continue  # drop Antarctica
        geometry = feat["geometry"]
        # 110m data is already simplified; keep as-is.
        countries.append(
            {
                "iso": iso,
                "admin": p.get("ADMIN") or iso,
                "pop": p.get("POP_EST") or 0,
                "region": p.get("REGION_UN") or "World",
                "geometry": geometry,
            }
        )
    return countries


COUNTRIES = load_ne()

# ---------------------------------------------------------------------------
# Fallback region groups so every polygon is owned by something in every era.
# ---------------------------------------------------------------------------
REGION_OWNER = {
    "Africa": "AFR",
    "Americas": "AME",
    "Asia": "ASI",
    "Europe": "EUR",
    "Oceania": "OCE",
}

# ---------------------------------------------------------------------------
# Era ownership maps: owner ISO3 code -> list of modern ISO3 territories.
# Only major states are listed; anything unmapped falls back to its region.
# ---------------------------------------------------------------------------

ERAS = {
    "03200_BCE": {
        "SUM": ["IRQ", "SYR"],
        "EGY": ["EGY"],
        "IND": ["PAK", "IND"],
        "XIA": ["CHN"],
    },
    "01500_BCE": {
        "EGY": ["EGY"],
        "HIT": ["TUR", "SYR"],
        "BAB": ["IRQ"],
        "SHA": ["CHN"],
        "CRT": ["GRC"],
        "IVA": ["PAK", "IND"],
    },
    "00500_BCE": {
        "ACH": ["IRN", "IRQ", "TUR", "SYR", "EGY", "AFG", "PAK", "ISR", "JOR", "LBN", "KWT", "QAT", "BHR", "ARE", "OMN"],
        "ATH": ["GRC"],
        "SPT": ["ITA"],
        "MAU": ["TUN", "DZA", "MAR", "LBY"],
        "MAU2": ["IND"],
        "ZHOU": ["CHN"],
    },
    "00001_CE": {
        "ROM": ["ITA", "FRA", "ESP", "PRT", "GBR", "GRC", "TUR", "EGY", "SYR", "LBY", "TUN", "DZA", "MAR", "ISR", "JOR", "LBN", "CHE", "AUT", "HUN", "ROU", "BGR", "HRV", "BIH", "SRB", "MNE", "ALB", "MKD", "BEL", "NLD", "LUX", "DEU"],
        "PAR": ["IRN", "IRQ", "AFG", "PAK"],
        "KUS": ["IND", "TJK", "UZB"],
        "HAN": ["CHN", "VNM", "KOR", "TWN"],
        "NAB": ["SAU", "YEM", "OMN", "JOR"],
        "MAU": ["MLI", "MRT", "SEN", "GMB", "GIN"],
    },
    "00500_CE": {
        "BYZ": ["GRC", "TUR", "SYR", "ISR", "JOR", "LBN", "EGY", "LBY", "TUN", "BGR", "ROU", "MKD", "SRB", "BIH"],
        "SAS": ["IRN", "IRQ", "AFG", "PAK"],
        "GUPTA": ["IND"],
        "SUI": ["CHN"],
        "FRA": ["FRA", "BEL", "NLD", "DEU", "CHE", "AUT"],
        "VIS": ["ESP", "PRT"],
        "OST": ["ITA", "HRV", "SVN"],
        "ANG": ["GBR"],
        "AKS": ["ERI", "ETH", "DJI"],
        "GHA": ["MRT", "MLI", "SEN"],
    },
    "01000_CE": {
        "BYZ": ["GRC", "TUR", "BGR", "ROU", "SYR", "ISR", "LBN"],
        "FAT": ["EGY", "TUN", "LBY", "SYR", "ISR", "JOR"],
        "HRF": ["FRA", "DEU", "CHE", "AUT", "ITA", "BEL", "NLD", "LUX", "HRV", "SVN", "CZE", "POL", "DNK", "NOR", "SWE"],
        "UMM": ["ESP", "PRT"],
        "KIE": ["UKR", "BLR", "RUS"],
        "ANG": ["GBR"],
        "SUG": ["CHN"],
        "GHA": ["MRT", "MLI", "SEN", "GIN", "GMB"],
        "SEL": ["IRN", "IRQ", "AFG", "TKM"],
    },
    "01300_CE": {
        "MGL": ["CHN", "MNG", "KOR", "RUS"],
        "MAM": ["EGY", "LBY", "ISR", "JOR", "SYR"],
        "OTT": ["TUR", "BGR", "GRC", "MKD", "SRB", "BIH", "ALB"],
        "FRA": ["FRA", "BEL", "NLD", "CHE"],
        "ENG": ["GBR"],
        "CAST": ["ESP"],
        "PAP": ["ITA"],
        "HRF": ["DEU", "AUT", "CZE", "POL", "DNK", "SWE", "NOR", "FIN", "HUN", "HRV", "SVN"],
        "MAL": ["MLI", "MRT", "SEN", "GIN", "GMB", "NER"],
        "DEL": ["IND", "PAK"],
        "ILS": ["IRN", "IRQ"],
    },
    "01500_CE": {
        "OTT": ["TUR", "GRC", "BGR", "ROU", "SRB", "BIH", "MKD", "ALB", "SYR", "ISR", "JOR", "EGY", "LBY", "TUN", "DZA", "SAU", "IRQ"],
        "SAF": ["IRN", "AFG", "AZE"],
        "MNG": ["CHN", "TWN", "VNM"],
        "MUG": ["IND", "PAK"],
        "SPA": ["ESP", "PRT", "NLD", "BEL", "ITA", "CHE", "AUT", "DEU", "CZE", "HRV", "MEX", "PER", "ARG", "CHL", "COL", "VEN", "CUB", "PHL"],
        "FRA": ["FRA"],
        "ENG": ["GBR"],
        "HRF": ["POL", "DNK", "SWE", "NOR", "FIN", "HUN", "ROU"],
        "RUS": ["RUS"],
        "INC": ["ECU", "PER", "BOL", "CHL", "ARG"],
        "AZT": ["MEX"],
    },
    "01700_CE": {
        "OTT": ["TUR", "GRC", "BGR", "ROU", "SRB", "BIH", "MKD", "ALB", "SYR", "ISR", "JOR", "EGY", "LBY", "TUN", "DZA", "SAU", "IRQ", "KWT"],
        "SAF": ["IRN", "AFG", "AZE", "TKM", "UZB"],
        "QIN": ["CHN", "TWN", "MNG", "TIB", "VNM"],
        "MUG": ["IND", "PAK", "BGD"],
        "FRA": ["FRA"],
        "SPA": ["ESP", "ITA", "MEX", "PER", "ARG", "CHL", "COL", "VEN", "CUB", "PHL", "PRT", "BEL", "NLD"],
        "ENG": ["GBR", "IRL"],
        "HRF": ["DEU", "AUT", "CZE", "HUN", "HRV", "POL", "DNK", "BEL", "LUX"],
        "RUS": ["RUS", "EST", "LVA", "LTU", "FIN"],
        "SWE": ["SWE", "NOR", "FIN"],
        "DUT": ["IDN", "LKA"],
        "FRA_NEW": [],
    },
    "01800_CE": {
        "FRN": ["FRA", "BEL", "NLD", "ITA", "CHE", "AUT", "DEU", "POL", "HRV", "SVN", "ESP", "PRT", "MEX"],
        "GBR": ["GBR", "IRL", "IND", "PAK", "BGD", "CAN", "AUS", "NZL", "USA"],
        "RUS": ["RUS", "FIN", "EST", "LVA", "LTU", "POL", "UKR", "BLR"],
        "OTT": ["TUR", "GRC", "BGR", "ROU", "SRB", "MKD", "ALB", "BIH", "SYR", "ISR", "JOR", "EGY", "LBY", "TUN", "DZA", "SAU", "IRQ"],
        "PER": ["IRN", "AFG", "AZE"],
        "QIN": ["CHN", "TWN", "MNG"],
        "SWE": ["SWE", "NOR", "FIN"],
        "USA": ["USA"],
        "SPA": ["ESP", "ARG", "CHL", "COL", "PER", "VEN", "CUB", "PHL"],
    },
    "01900_CE": {
        "GBR": ["GBR", "IRL", "IND", "PAK", "BGD", "CAN", "AUS", "NZL", "NGA", "GHA", "KEN", "UGA", "ZMB", "ZWE", "SGP", "MMR", "LKA", "MYS"],
        "FRA": ["FRA", "ALG", "TUN", "MOR", "SEN", "MLI", "GIN", "NCL", "VNM", "LAO", "KHM", "DJI"],
        "GER": ["DEU", "POL", "CZE", "HRV", "SVN", "TGO", "CMR", "NAM", "PNG"],
        "RUS": ["RUS", "FIN", "EST", "LVA", "LTU", "POL", "UKR", "BLR", "KAZ", "UZB", "TKM", "KGZ", "TJK", "MNG"],
        "AUS": ["AUT", "HUN", "CZE", "SVK", "HRV", "BIH", "SRB", "ROU", "POL", "ITA"],
        "OTT": ["TUR", "GRC", "BGR", "ROU", "SRB", "MKD", "ALB", "BIH", "SYR", "ISR", "JOR", "IRQ", "KWT", "SAU", "LBY", "TUN", "DZA"],
        "USA": ["USA", "PRI", "PHL", "CUB", "GUM"],
        "JPN": ["JPN", "TWN"],
        "ITA": ["ITA", "LBY", "ERI", "SOM"],
        "QIN": ["CHN", "MNG"],
        "SPA": ["ESP", "MAR"],
        "NLD": ["NLD", "IDN"],
        "PRT": ["PRT", "AGO", "MOZ"],
        "BEL": ["BEL", "COD", "RWA", "BDI"],
        "SWE": ["SWE", "NOR"],
        "DNK": ["DNK", "ISL"],
        "USA2": [],
        "BRA": ["BRA"],
        "ARG": ["ARG"],
        "CHL": ["CHL"],
        "MEX": ["MEX"],
        "PER": ["PER"],
        "IRN": ["IRN", "AFG"],
        "SAU": ["SAU", "YEM", "OMN"],
        "ETH": ["ETH", "ERI"],
        "THA": ["THA"],
        "KOR": ["KOR"],
    },
    "01938_CE": {
        "GER": ["DEU", "AUT", "CZE", "POL", "DNK", "BEL", "NLD", "LUX", "FRA", "HRV", "SVN"],
        "ITA": ["ITA", "LBY", "ERI", "SOM", "ALB", "GRC"],
        "JPN": ["JPN", "TWN", "KOR", "CHN", "MMR", "THA", "VNM", "IDN", "PHL", "MYS"],
        "USSR": ["RUS", "UKR", "BLR", "KAZ", "UZB", "TKM", "KGZ", "TJK", "AZE", "ARM", "GEO", "MDA", "EST", "LVA", "LTU", "FIN"],
        "USA": ["USA"],
        "GBR": ["GBR", "IRL", "CAN", "AUS", "NZL", "IND", "PAK", "BGD", "NGA", "KEN", "EGY"],
        "FRA": ["FRA", "ALG", "TUN", "MOR", "SEN", "VNM", "LAO", "KHM"],
        "CHN": ["CHN", "MNG", "TWN"],
        "HUN": ["HUN", "ROU"],
        "BUL": ["BGR"],
        "YUG": ["SRB", "BIH", "MKD", "MNE"],
        "ROM": ["ROU"],
        "POL": ["POL"],
        "TUR": ["TUR"],
        "IRN": ["IRN", "AFG"],
        "SAU": ["SAU", "YEM", "OMN", "IRQ"],
        "ETH": ["ETH"],
        "ARG": ["ARG"],
        "BRA": ["BRA"],
        "MEX": ["MEX"],
        "SPA": ["ESP"],
        "PRT": ["PRT"],
        "SWE": ["SWE", "NOR"],
        "SUI": ["CHE"],
        "NLD": ["NLD", "IDN"],
        "BEL": ["BEL", "COD", "RWA", "BDI"],
        "THA": ["THA"],
    },
    "01945_CE": {
        "USA": ["USA", "PRI", "PHL", "GUM"],
        "USSR": ["RUS", "UKR", "BLR", "KAZ", "UZB", "TKM", "KGZ", "TJK", "AZE", "ARM", "GEO", "MDA", "EST", "LVA", "LTU", "MNG", "POL", "CZE", "SVK", "HUN", "ROU", "BGR", "FIN"],
        "GBR": ["GBR", "IRL", "CAN", "AUS", "NZL", "IND", "PAK", "BGD", "NGA", "KEN", "EGY", "ZAF"],
        "FRA": ["FRA", "ALG", "TUN", "MOR", "SEN", "VNM", "LAO", "KHM", "SYR", "LBN"],
        "CHN": ["CHN"],
        "GER": ["DEU", "AUT"],
        "ITA": ["ITA"],
        "JPN": ["JPN"],
        "YUG": ["SRB", "BIH", "MKD", "MNE", "HRV", "SVN"],
        "TUR": ["TUR"],
        "IRN": ["IRN"],
        "SAU": ["SAU", "YEM", "OMN", "IRQ"],
        "ETH": ["ETH", "ERI"],
        "ARG": ["ARG"],
        "BRA": ["BRA"],
        "MEX": ["MEX"],
        "SPA": ["ESP", "MAR"],
        "PRT": ["PRT", "MOZ", "AGO"],
        "NLD": ["NLD", "IDN"],
        "BEL": ["BEL", "COD", "RWA", "BDI"],
        "SWE": ["SWE", "NOR"],
        "SUI": ["CHE"],
        "DEN": ["DNK", "ISL"],
        "THA": ["THA"],
        "AUS": ["AUS"],
        "CAN": ["CAN"],
    },
    "01991_CE": {
        "USA": ["USA"],
        "USSR": ["RUS", "UKR", "BLR", "KAZ", "UZB", "TKM", "KGZ", "TJK", "AZE", "ARM", "GEO", "MDA", "EST", "LVA", "LTU"],
        "CHN": ["CHN", "MNG"],
        "GBR": ["GBR", "IRL", "CAN", "AUS", "NZL", "HKG"],
        "FRA": ["FRA"],
        "GER": ["DEU"],
        "JPN": ["JPN"],
        "ITA": ["ITA"],
        "IND": ["IND", "PAK", "BGD"],
        "BRA": ["BRA"],
        "MEX": ["MEX"],
        "ARG": ["ARG"],
        "SAU": ["SAU", "YEM", "OMN"],
        "IRN": ["IRN"],
        "TUR": ["TUR"],
        "EGY": ["EGY"],
        "ISR": ["ISR"],
        "ZAF": ["ZAF"],
        "YUG": ["SRB", "BIH", "MKD", "MNE", "HRV", "SVN"],
        "NGA": ["NGA"],
        "POL": ["POL"],
        "CZE": ["CZE", "SVK"],
        "HUN": ["HUN"],
        "ROU": ["ROU"],
        "BGR": ["BGR"],
        "GRC": ["GRC"],
        "ESP": ["ESP"],
        "PRT": ["PRT"],
        "NLD": ["NLD"],
        "BEL": ["BEL"],
        "SWE": ["SWE"],
        "NOR": ["NOR"],
        "FIN": ["FIN"],
        "DNK": ["DNK"],
        "SUI": ["CHE"],
        "AUT": ["AUT"],
        "UKR": ["UKR"],
    },
}

# Modern 2020 baseline uses each country as its own nation (accurate).
MODERN_2020 = None  # generated from NE below


# ---------------------------------------------------------------------------
# Religion/ethnicity/tech metadata per owner for era baselines.
# ---------------------------------------------------------------------------

DEFAULT_RELIGION = {"Traditional/Unspecified": 100.0}
DEFAULT_ETHNICITY = {"Unspecified": 100.0}

ERA_TECHS = {
    "03200_BCE": [("Writing (cuneiform)", "writing", 3200, 0.01)],
    "01500_BCE": [("Bronze metallurgy", "metallurgy", 2500, 0.3), ("Wheeled vehicles", "transport", 3000, 0.2)],
    "00500_BCE": [("Iron metallurgy", "metallurgy", 1200, 0.5), ("Alphabet", "writing", 1050, 0.4), ("Road networks", "transport", 500, 0.3)],
    "00001_CE": [("Aqueducts", "engineering", 100, 0.3), ("Concrete", "engineering", 50, 0.3), ("Paper", "writing", 105, 0.2)],
    "00500_CE": [("Horse stirrup", "military", 400, 0.4), ("Block printing", "writing", 600, 0.2)],
    "01000_CE": [("Gunpowder", "military", 900, 0.2), ("Compass", "navigation", 1000, 0.3)],
    "01300_CE": [("Mechanical clocks", "engineering", 1280, 0.2), ("Rocketry", "military", 1230, 0.1)],
    "01500_CE": [("Printing press", "writing", 1440, 0.6), ("Astrolabe navigation", "navigation", 1400, 0.5)],
    "01700_CE": [("Telescope", "science", 1608, 0.5), ("Steam engine", "energy", 1698, 0.3), ("Microscope", "science", 1590, 0.4)],
    "01800_CE": [("Steam power", "energy", 1765, 0.7), ("Vaccination", "medical", 1796, 0.4), ("Textile machinery", "industry", 1764, 0.7)],
    "01900_CE": [("Electricity grids", "energy", 1882, 0.7), ("Telephone", "communication", 1876, 0.7), ("Internal combustion", "transport", 1876, 0.6), ("Radio", "communication", 1895, 0.4)],
    "01938_CE": [("Aviation", "transport", 1903, 0.8), ("Radar", "military", 1935, 0.4), ("Antibiotics", "medical", 1928, 0.5)],
    "01945_CE": [("Nuclear fission", "energy", 1942, 0.3), ("Jet engine", "transport", 1944, 0.5), ("Electronic computer", "computing", 1945, 0.3)],
    "01991_CE": [("Spaceflight", "space", 1957, 0.8), ("Integrated circuit", "computing", 1958, 0.9), ("Internet", "communication", 1969, 0.8)],
    "02020_CE": [("World Wide Web", "communication", 1991, 1.0), ("Smartphones", "computing", 2007, 1.0), ("Machine learning", "computing", 2012, 0.9), ("Reusable rockets", "space", 2015, 0.6)],
}

# Approximate population (millions) for major historical owners.
HIST_POP = {
    "SUM": 0.5, "EGY": 1.5, "IND": 10, "XIA": 3, "HIT": 2, "BAB": 1.5, "SHA": 5,
    "CRT": 0.3, "IVA": 5, "ACH": 17, "ATH": 0.5, "SPT": 3, "MAU": 4, "MAU2": 12,
    "ZHOU": 20, "ROM": 45, "PAR": 7, "KUS": 12, "HAN": 60, "NAB": 1, "BYZ": 20,
    "SAS": 17, "GUPTA": 30, "SUI": 50, "FRA": 8, "VIS": 4, "OST": 5, "ANG": 2,
    "AKS": 1, "GHA": 2, "FAT": 5, "HRF": 8, "UMM": 6, "KIE": 4, "SUG": 60,
    "SEL": 5, "MGL": 100, "MAM": 5, "OTT": 12, "ENG": 4, "CAST": 6, "PAP": 2,
    "MAL": 3, "DEL": 20, "ILS": 8, "SAF": 6, "MNG": 80, "MUG": 110, "SPA": 25,
    "RUS": 12, "INC": 8, "AZT": 6, "QIN": 250, "DUT": 2, "SWE": 2, "FRN": 35,
    "PER": 6, "USA": 5, "GBR": 30, "GER": 40, "AUS": 40, "JPN": 40, "ITA": 25,
    "NLD": 5, "BEL": 6, "PRT": 5, "DNK": 2, "BRA": 20, "ARG": 10, "CHL": 4,
    "MEX": 15, "IRN": 12, "SAU": 6, "ETH": 10, "THA": 10, "KOR": 15, "CHN": 450,
    "HUN": 10, "BUL": 6, "YUG": 15, "ROM": 16, "POL": 30, "TUR": 15, "USSR": 190,
    "SUI": 4, "DEN": 4, "IND": 350, "ISR": 2, "ZAF": 15, "NGA": 40, "CZE": 15,
    "GRC": 7, "ESP": 20, "FIN": 3, "NOR": 2, "UKR": 40, "AFR": 5, "AME": 3,
    "ASI": 5, "EUR": 3, "OCE": 1,
}


def region_code(iso):
    for c in COUNTRIES:
        if c["iso"] == iso:
            return REGION_OWNER.get(c["region"], "ASI")
    return "ASI"


# ---------------------------------------------------------------------------
# Event builders (payloads must match the Rust serde enum, snake_case kinds).
# ---------------------------------------------------------------------------

def narrative_payload(text):
    return {"kind": "narrative", "text": text}


def baseline_payload(nations, territories, techs):
    return {
        "kind": "epoch_baseline",
        "nations": nations,
        "territories": territories,
        "techs": techs,
    }


def techs_payload(era_key, date):
    out = []
    for name, cat, year, adoption in ERA_TECHS.get(era_key, []):
        out.append({
            "tech_id": name.lower().replace(" ", "_"),
            "name": name,
            "category": cat,
            "invented": {"year": year, "month": 1, "day": 1},
            "adoption": adoption,
        })
    return out


def color_hash(owner):
    pal = [
        "#e6194b", "#3cb44b", "#ffe119", "#4363d8", "#f58231", "#911eb4",
        "#46f0f0", "#f032e6", "#bcf60c", "#fabebe", "#008080", "#e6beff",
        "#9a6324", "#fffac8", "#800000", "#aaffc3", "#808000", "#ffd8b1",
        "#000075", "#808080",
    ]
    h = 5381
    for ch in owner:
        h = ((h * 33) + ord(ch)) & 0xFFFFFFFF
    return pal[h % len(pal)]


def make_baseline(era_key, date, owner_map):
    """Build nations + territories for an era using modern polygons."""
    # Determine which modern countries map to which owner (explicit or region).
    mapped = {}  # iso -> owner
    for owner, terrs in owner_map.items():
        for t in terrs:
            mapped[t] = owner
    nations = {}
    territories = []
    for c in COUNTRIES:
        iso = c["iso"]
        owner = mapped.get(iso, region_code(iso))
        # Accumulate geometry by owner: keep per-country polygons (simplest).
        if owner not in nations:
            nations[owner] = {
                "id": owner,
                "name": owner,
                "color": color_hash(owner),
                "population": int(HIST_POP.get(owner, 1) * 1_000_000),
                "religion_pct": [["Traditional/Unspecified", 100.0]],
                "ethnicity_pct": [["Unspecified", 100.0]],
                "economy_index": 10.0,
                "military_index": 10.0,
                "territories": [],
            }
        territories.append({
            "id": iso,
            "name": c["admin"],
            "owner": owner,
            "geometry_geojson": json.dumps(c["geometry"]),
        })
    return {
        "nations": list(nations.values()),
        "territories": territories,
    }


def make_modern_2020():
    nations = []
    territories = []
    for c in COUNTRIES:
        iso = c["iso"]
        nations.append({
            "id": iso,
            "name": c["admin"],
            "color": color_hash(iso),
            "population": int(c["pop"]),
            "religion_pct": [["Multiple/Unspecified", 100.0]],
            "ethnicity_pct": [["Multiple/Unspecified", 100.0]],
            "economy_index": 50.0,
            "military_index": 20.0,
            "territories": [],
        })
        territories.append({
            "id": iso,
            "name": c["admin"],
            "owner": iso,
            "geometry_geojson": json.dumps(c["geometry"]),
        })
    return {"nations": nations, "territories": territories}


# ---------------------------------------------------------------------------
# Paleolithic and milestone narrative events (real history).
# ---------------------------------------------------------------------------

MILESTONES = [
    (-2_000_000, "Homo erectus appears",
     "Homo erectus emerges in East Africa. They will be the first human species to \
control fire, cook food, and migrate out of Africa into Eurasia."),
    (-1_900_000, "First controlled fire",
     "The earliest evidence of controlled use of fire dates to this era \
(Koobi Fora, Kenya). Cooked food transforms human energy budgets and fuels brain growth."),
    (-1_700_000, "Homo erectus leaves Africa",
     "Homo erectus populations move into the Levant, the Caucasus, and across Asia \
(Dmanisi, Georgia is the oldest known site outside Africa)."),
    (-1_000_000, "Fire use becomes habitual",
     "Across Africa and Eurasia, habitual fire use spreads; hearths, light, and \
protection from predators reshape human daily life."),
    (-500_000, "Homo heidelbergensis",
     "A large-brained descendant of Homo erectus, ancestral to both Neanderthals and \
modern humans, thrives across Africa and Europe."),
    (-300_000, "Homo sapiens appears",
     "Anatomically modern humans evolve in Africa. Around the same era Neanderthals \
thrive across Europe and the Middle East, and Denisovans across Asia."),
    (-200_000, "Neanderthal domination of Europe",
     "Neanderthals, adept hunters adapted to cold, dominate Europe and western Asia."),
    (-70_000, "Homo sapiens leaves Africa again",
     "Modern humans expand out of Africa in the great dispersal; within tens of \
thousands of years they reach every habitable continent."),
    (-60_000, "Colonization of Asia and Australia",
     "Modern humans sweep across southern Asia and, by sea crossing, reach \
Australia, demonstrating advanced maritime capability."),
    (-45_000, "Cave art and symbolic thought",
     "The earliest cave paintings appear (Indonesia, later Europe); symbol use and \
ritual burials signal fully modern cognition."),
    (-40_000, "Homo sapiens enters Europe",
     "Modern humans enter Europe and begin the gradual replacement and \
interbreeding with Neanderthals."),
    (-30_000, "Neanderthals go extinct",
     "The last Neanderthals vanish; every living human outside Africa carries \
a small Neanderthal inheritance from admixture."),
    (-15_000, "Beringia and the peopling of the Americas",
     "Humans cross Beringia and spread through the Americas, reaching the southern \
tip of South America within a few millennia."),
    (-10_000, "Agricultural Revolution begins",
     "The first cultivation and domestication begin in the Fertile Crescent \
(wheat, barley, goats, sheep), followed by rice in China and maize in Mesoamerica. \
Permanent villages replace nomadic life."),
    (-7_000, "First cities",
     "Jericho and Çatalhöyük emerge as large permanent settlements; population \
density and social complexity rise."),
    (-3_300, "First cities of Sumer",
     "Uruk grows into the world's first true city; temple economies, trade \
networks, and social hierarchies form."),
    (-3_200, "Invention of writing",
     "The Sumerians develop cuneiform, the world's first writing system, for \
accounting and administration at Uruk. Recorded history begins."),
    (-3_100, "Unification of Egypt",
     "Upper and Lower Egypt are united; the First Dynasty rules from Memphis. \
Writing (hieroglyphs) develops independently."),
    (-2_500, "Great Pyramid of Giza",
     "The Great Pyramid is completed, the tallest human structure for nearly \
four thousand years."),
    (-2_000, "Indus Valley civilization at its height",
     "Mohenjo-daro and Harappa flourish with planned streets, drainage, and \
standardized weights; trade reaches Mesopotamia."),
    (-1_200, "Iron Age begins",
     "Iron metallurgy spreads from Anatolia, transforming weapons, agriculture, \
and every subsequent civilization."),
    (-776, "First Olympic Games",
     "Panhellenic games begin at Olympia, a rare moment of unity among warring \
Greek city-states."),
    (-551, "Confucius is born",
     "Kong Qiu is born in Lu (China); his philosophy will shape East Asian \
society for two and a half millennia."),
    (-500, "First Democracy",
     "Cleisthenes' reforms establish the first democracy in Athens."),
    (-221, "Qin unifies China",
     "Qin Shi Huang unifies China, standardizes writing, weights, and measures, \
and begins the Great Wall."),
    (0, "Common Era begins",
     "The traditional birth year of Jesus of Nazareth marks the start of the \
common era in the West."),
    (476, "Fall of the Western Roman Empire",
     "Odoacer deposes the last Western Roman emperor; the Middle Ages begin \
in Europe."),
    (622, "Hijra and the birth of Islam",
     "Muhammad's migration to Medina founds the first Muslim community; within \
a century Islam spans three continents."),
    (800, "Charlemagne crowned Emperor",
     "The coronation of Charlemagne restores a Western empire and anchors \
medieval Europe."),
    (1040, "The compass in navigation",
     "Chinese mariners use the magnetic compass; global navigation is \
transformed."),
    (1258, "Mongol conquest of Baghdad",
     "The Abbasid Caliphate falls to the Mongols; the Islamic world's center \
of gravity shifts to Cairo and Iran."),
    (1439, "Gutenberg's printing press",
     "Movable-type printing arrives in Europe, seeding the Renaissance and the \
scientific revolution."),
    (1492, "Columbus reaches the Americas",
     "The Columbian exchange begins: two previously separate worlds merge, \
with devastating and transformative consequences."),
    (1776, "American Revolution",
     "The Thirteen Colonies declare independence; republican revolution \
becomes a global template."),
    (1789, "French Revolution",
     "The Bastille falls; Europe's old order is shaken and the age of \
nationhood and ideology begins."),
    (1914, "World War I begins",
     "The assassination of Archduke Franz Ferdinand ignites the Great War, \
reshaping every great power."),
    (1939, "World War II begins",
     "Germany invades Poland; within six years the war will kill tens of \
millions and redraw the world."),
    (1945, "Atomic age begins",
     "Trinity test and Hiroshima/Nagasaki introduce nuclear weapons; the Cold \
War follows within two years."),
    (1947, "India independent; Cold War dawns",
     "Decolonization accelerates worldwide even as the United States and the \
Soviet Union divide the globe."),
    (1969, "Moon landing",
     "Apollo 11 lands humans on the Moon, the furthest humans have ever \
traveled."),
    (1991, "Soviet Union dissolves",
     "The Cold War ends; the United States stands as the sole superpower and \
globalization accelerates."),
    (2001, "September 11 and the war on terror",
     "The 9/11 attacks redefine international security for a generation."),
    (2020, "COVID-19 pandemic",
     "A global pandemic forces unprecedented shutdowns and accelerates \
digital transformation worldwide."),
]


# ---------------------------------------------------------------------------
# Main build
# ---------------------------------------------------------------------------

def build():
    if not os.path.exists(RAW):
        print("missing Natural Earth data at", RAW)
        sys.exit(1)
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    if os.path.exists(OUT):
        os.remove(OUT)

    conn = sqlite3.connect(OUT)
    conn.executescript(
        """
        CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE canonical_events (
            id INTEGER PRIMARY KEY,
            date_day INTEGER NOT NULL,
            date_year INTEGER NOT NULL,
            date_month INTEGER NOT NULL,
            date_dayofmonth INTEGER NOT NULL,
            title TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '',
            payload TEXT NOT NULL,
            seq INTEGER NOT NULL
        );
        CREATE INDEX idx_canon_date ON canonical_events(date_day);
        """
    )

    events = []
    seq = 0

    def add(date_tuple, title, body, payload, seq_id=None):
        nonlocal seq
        seq += 1
        y, m, d = date_tuple
        events.append(
            (
                seq_id if seq_id else seq,
                days_from_ce(y, m, d),
                y, m, d,
                title, body, json.dumps(payload),
                seq,
            )
        )

    # --- Paleolithic + milestone narrative events (real history) -----------
    for year, title, body in MILESTONES:
        add((year, 1, 1), title, body, narrative_payload(body))

    # --- Era baselines -----------------------------------------------------
    era_dates = {
        "03200_BCE": (-3199, 1, 1),
        "01500_BCE": (-1499, 1, 1),
        "00500_BCE": (-499, 1, 1),
        "00001_CE": (1, 1, 1),
        "00500_CE": (500, 1, 1),
        "01000_CE": (1000, 1, 1),
        "01300_CE": (1300, 1, 1),
        "01500_CE": (1500, 1, 1),
        "01700_CE": (1700, 1, 1),
        "01800_CE": (1800, 1, 1),
        "01900_CE": (1900, 1, 1),
        "01938_CE": (1938, 1, 1),
        "01945_CE": (1945, 1, 1),
        "01991_CE": (1991, 1, 1),
    }
    era_dates.pop("02020_CE", None)

    for era_key, date_tuple in era_dates.items():
        owner_map = ERAS[era_key]
        base = make_baseline(era_key, date_tuple, owner_map)
        payload = baseline_payload(
            base["nations"],
            base["territories"],
            techs_payload(era_key, date_tuple),
        )
        add(date_tuple, f"Baseline: {era_key.replace('_', ' ')}",
            "World snapshot at this date.", payload)

    # Modern 2020 accurate baseline (each country its own nation).
    base = make_modern_2020()
    payload = baseline_payload(
        base["nations"],
        base["territories"],
        techs_payload("02020_CE", (2020, 1, 1)),
    )
    add((2020, 1, 1), "Baseline: Modern world (2020)",
        "Accurate modern world from Natural Earth country data.", payload)

    # --- Write ------------------------------------------------------------
    conn.executemany(
        "INSERT INTO canonical_events (id, date_day, date_year, date_month, "
        "date_dayofmonth, title, body, payload, seq) "
        "VALUES (?,?,?,?,?,?,?,?,?)",
        events,
    )
    conn.execute("INSERT INTO meta VALUES ('seed_version', '1.0.0-paleolithic')")
    conn.commit()
    conn.close()

    print(f"built {OUT}")
    print(f"  events: {len(events)}")
    print(f"  first: {events[0][2]}-{events[0][3]:02d}-{events[0][4]:02d} {events[0][5]}")
    print(f"  last:  {events[-1][2]}-{events[-1][3]:02d}-{events[-1][4]:02d} {events[-1][5]}")


if __name__ == "__main__":
    build()
