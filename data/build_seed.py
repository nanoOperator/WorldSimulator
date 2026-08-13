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
    "03200_BCE": [
        ("Writing (cuneiform)", "writing", 3200, 0.01),
        ("Ox-drawn plows", "agriculture", 3300, 0.3),
        ("Sailing ships", "transport", 3200, 0.2),
        ("Copper smelting", "metallurgy", 3500, 0.4),
    ],
    "01500_BCE": [
        ("Bronze metallurgy", "metallurgy", 2500, 0.3),
        ("Wheeled vehicles", "transport", 3000, 0.2),
        ("Chariot warfare", "military", 2000, 0.3),
        ("Glassmaking", "engineering", 2000, 0.15),
    ],
    "00500_BCE": [
        ("Iron metallurgy", "metallurgy", 1200, 0.5),
        ("Alphabet", "writing", 1050, 0.4),
        ("Road networks", "transport", 500, 0.3),
        ("Coinage", "economy", 600, 0.35),
        ("Qanats (underground canals)", "engineering", 700, 0.2),
        ("Crossbows", "military", 500, 0.15),
    ],
    "00001_CE": [
        ("Aqueducts", "engineering", 100, 0.3),
        ("Concrete", "engineering", 50, 0.3),
        ("Paper", "writing", 105, 0.2),
        ("Waterwheels", "energy", 100, 0.2),
        ("Canal locks", "engineering", 200, 0.1),
    ],
    "00500_CE": [
        ("Horse stirrup", "military", 400, 0.4),
        ("Block printing", "writing", 600, 0.2),
        ("Moldboard plow", "agriculture", 500, 0.3),
        ("Decimal numeral system", "science", 500, 0.4),
    ],
    "01000_CE": [
        ("Gunpowder", "military", 900, 0.2),
        ("Compass", "navigation", 1000, 0.3),
        ("Paper money", "economy", 1000, 0.2),
        ("Moveable type (China)", "writing", 1040, 0.1),
        ("Windmills", "energy", 900, 0.2),
    ],
    "01300_CE": [
        ("Mechanical clocks", "engineering", 1280, 0.2),
        ("Rocketry", "military", 1230, 0.1),
        ("Cannons", "military", 1300, 0.15),
        ("Oceangoing junks", "transport", 1300, 0.3),
    ],
    "01500_CE": [
        ("Printing press", "writing", 1440, 0.6),
        ("Astrolabe navigation", "navigation", 1400, 0.5),
        ("Firearms (arquebus)", "military", 1450, 0.3),
        ("Caravels", "transport", 1450, 0.4),
    ],
    "01700_CE": [
        ("Telescope", "science", 1608, 0.5),
        ("Steam engine", "energy", 1698, 0.3),
        ("Microscope", "science", 1590, 0.4),
        ("Newtonian mechanics", "science", 1687, 0.5),
        ("Barometer", "science", 1643, 0.3),
    ],
    "01800_CE": [
        ("Steam power", "energy", 1765, 0.7),
        ("Vaccination", "medical", 1796, 0.4),
        ("Textile machinery", "industry", 1764, 0.7),
        ("Railways", "transport", 1825, 0.6),
        ("Telegraph", "communication", 1844, 0.6),
        ("Photography", "science", 1839, 0.2),
        ("Steamships", "transport", 1807, 0.4),
    ],
    "01900_CE": [
        ("Electricity grids", "energy", 1882, 0.7),
        ("Telephone", "communication", 1876, 0.7),
        ("Internal combustion", "transport", 1876, 0.6),
        ("Radio", "communication", 1895, 0.4),
        ("Assembly line", "industry", 1913, 0.6),
        ("Airplanes", "transport", 1903, 0.5),
        ("X-rays", "medical", 1895, 0.4),
    ],
    "01938_CE": [
        ("Aviation", "transport", 1903, 0.8),
        ("Radar", "military", 1935, 0.4),
        ("Antibiotics", "medical", 1928, 0.5),
        ("Synthetic rubber", "industry", 1930, 0.4),
        ("Television", "communication", 1935, 0.3),
    ],
    "01945_CE": [
        ("Nuclear fission", "energy", 1942, 0.3),
        ("Jet engine", "transport", 1944, 0.5),
        ("Electronic computer", "computing", 1945, 0.3),
        ("Transistor", "computing", 1947, 0.4),
        ("Rockets (ballistic)", "space", 1944, 0.3),
    ],
    "01991_CE": [
        ("Spaceflight", "space", 1957, 0.8),
        ("Integrated circuit", "computing", 1958, 0.9),
        ("Internet", "communication", 1969, 0.8),
        ("Personal computers", "computing", 1975, 0.7),
        ("Mobile phones", "communication", 1983, 0.6),
        ("GPS", "navigation", 1978, 0.6),
    ],
    "02020_CE": [
        ("World Wide Web", "communication", 1991, 1.0),
        ("Smartphones", "computing", 2007, 1.0),
        ("Machine learning", "computing", 2012, 0.9),
        ("Reusable rockets", "space", 2015, 0.6),
        ("Renewable energy", "energy", 2010, 0.7),
        ("CRISPR gene editing", "medical", 2013, 0.4),
        ("Electric vehicles", "transport", 2012, 0.6),
        ("Satellite internet", "communication", 2019, 0.5),
        ("Cloud computing", "computing", 2006, 0.9),
        ("Social media", "communication", 2006, 0.95),
        ("5G networks", "communication", 2019, 0.6),
        ("mRNA vaccines", "medical", 2020, 0.5),
        ("3D printing", "industry", 2010, 0.5),
        ("Drones", "military", 2010, 0.7),
        ("Blockchain", "computing", 2009, 0.4),
        ("Autonomous vehicles", "transport", 2016, 0.3),
        ("Quantum computing", "computing", 2019, 0.1),
        ("Fusion research", "energy", 2020, 0.05),
    ],
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

# Religion composition (%) per era for headline owners. Owners not listed fall
# back to Traditional/Unspecified. Sums approximate 100.
RELIGION_BY_ERA = {
    "00001_CE": {
        "ROM": [["Roman polytheism", 78], ["Christianity", 2], ["Judaism", 4], ["Mithraism", 3], ["Other", 13]],
        "HAN": [["Chinese folk", 88], ["Buddhism", 3], ["Other", 9]],
        "KUS": [["Buddhism", 40], ["Hinduism", 30], ["Other", 30]],
        "PAR": [["Zoroastrianism", 70], ["Other", 30]],
        "NAB": [["Arabian polytheism", 80], ["Judaism", 5], ["Christianity", 3], ["Other", 12]],
        "MAU": [["Traditional", 100]],
    },
    "00500_CE": {
        "BYZ": [["Christianity", 88], ["Judaism", 3], ["Other", 9]],
        "SAS": [["Zoroastrianism", 60], ["Christianity", 15], ["Manichaeism", 10], ["Buddhism", 5], ["Other", 10]],
        "GUPTA": [["Hinduism", 70], ["Buddhism", 20], ["Jainism", 5], ["Other", 5]],
        "SUI": [["Buddhism", 55], ["Chinese folk", 30], ["Taoism", 10], ["Other", 5]],
        "FRA": [["Christianity", 70], ["Pagan", 25], ["Other", 5]],
        "VIS": [["Christianity", 55], ["Pagan", 35], ["Other", 10]],
        "OST": [["Christianity", 65], ["Pagan", 30], ["Other", 5]],
        "ANG": [["Christianity", 75], ["Pagan", 20], ["Other", 5]],
        "AKS": [["Christianity", 60], ["Traditional", 35], ["Other", 5]],
        "GHA": [["Traditional", 100]],
    },
    "01000_CE": {
        "BYZ": [["Christianity", 90], ["Islam", 5], ["Judaism", 2], ["Other", 3]],
        "FAT": [["Islam", 90], ["Christianity", 6], ["Judaism", 2], ["Other", 2]],
        "UMM": [["Islam", 85], ["Christianity", 8], ["Judaism", 3], ["Other", 4]],
        "HRF": [["Christianity", 88], ["Islam", 5], ["Other", 7]],
        "KIE": [["Christianity", 70], ["Slavic paganism", 25], ["Other", 5]],
        "SUG": [["Buddhism", 60], ["Chinese folk", 25], ["Taoism", 10], ["Other", 5]],
        "GHA": [["Traditional", 100]],
        "SEL": [["Islam", 80], ["Christianity", 10], ["Zoroastrianism", 5], ["Other", 5]],
    },
    "01300_CE": {
        "MGL": [["Shamanism", 45], ["Buddhism", 30], ["Other", 25]],
        "MAM": [["Islam", 92], ["Christianity", 5], ["Other", 3]],
        "OTT": [["Islam", 85], ["Christianity", 10], ["Other", 5]],
        "FRA": [["Christianity", 92], ["Judaism", 2], ["Other", 6]],
        "ENG": [["Christianity", 92], ["Other", 8]],
        "CAST": [["Christianity", 90], ["Islam", 5], ["Judaism", 3], ["Other", 2]],
        "PAP": [["Christianity", 95], ["Other", 5]],
        "HRF": [["Christianity", 90], ["Judaism", 2], ["Other", 8]],
        "MAL": [["Islam", 40], ["Traditional", 55], ["Other", 5]],
        "DEL": [["Hinduism", 70], ["Islam", 20], ["Buddhism", 5], ["Other", 5]],
        "ILS": [["Islam", 85], ["Christianity", 5], ["Zoroastrianism", 5], ["Other", 5]],
    },
    "01500_CE": {
        "OTT": [["Islam", 88], ["Christianity", 8], ["Judaism", 2], ["Other", 2]],
        "SAF": [["Islam (Shia)", 85], ["Islam (Sunni)", 8], ["Christianity", 3], ["Other", 4]],
        "MNG": [["Buddhism", 65], ["Chinese folk", 25], ["Taoism", 7], ["Other", 3]],
        "MUG": [["Islam", 55], ["Hinduism", 35], ["Sikhism", 2], ["Other", 8]],
        "SPA": [["Christianity (Catholic)", 92], ["Islam", 3], ["Judaism", 2], ["Other", 3]],
        "FRA": [["Christianity", 94], ["Other", 6]],
        "ENG": [["Christianity", 95], ["Other", 5]],
        "RUS": [["Christianity", 90], ["Islam", 5], ["Other", 5]],
        "INC": [["Traditional (Inca)", 100]],
        "AZT": [["Traditional (Aztec)", 100]],
    },
    "01700_CE": {
        "OTT": [["Islam", 90], ["Christianity", 6], ["Judaism", 2], ["Other", 2]],
        "SAF": [["Islam (Shia)", 90], ["Islam (Sunni)", 5], ["Other", 5]],
        "QIN": [["Chinese folk", 60], ["Buddhism", 20], ["Taoism", 15], ["Other", 5]],
        "MUG": [["Islam", 60], ["Hinduism", 30], ["Sikhism", 3], ["Other", 7]],
        "FRN": [["Christianity (Catholic)", 90], ["Protestantism", 5], ["Other", 5]],
        "ENG": [["Christianity (Anglican)", 85], ["Protestantism", 10], ["Other", 5]],
        "SPA": [["Christianity (Catholic)", 95], ["Other", 5]],
        "RUS": [["Christianity (Orthodox)", 88], ["Islam", 6], ["Other", 6]],
        "SWE": [["Christianity (Lutheran)", 92], ["Other", 8]],
        "DUT": [["Christianity", 85], ["Islam", 4], ["Other", 11]],
        "INC": [["Traditional (Inca)", 100]],
        "AZT": [["Traditional (Aztec)", 100]],
    },
    "01800_CE": {
        "FRN": [["Christianity (Catholic)", 85], ["Protestantism", 8], ["Other", 7]],
        "GBR": [["Christianity (Anglican)", 80], ["Protestantism", 10], ["Other", 10]],
        "RUS": [["Christianity (Orthodox)", 85], ["Islam", 8], ["Other", 7]],
        "OTT": [["Islam", 85], ["Christianity", 10], ["Judaism", 3], ["Other", 2]],
        "PER": [["Islam (Shia)", 88], ["Islam (Sunni)", 7], ["Other", 5]],
        "QIN": [["Chinese folk", 65], ["Buddhism", 18], ["Taoism", 12], ["Other", 5]],
        "SWE": [["Christianity (Lutheran)", 93], ["Other", 7]],
        "USA": [["Christianity (Protestant)", 65], ["Christianity (Catholic)", 20], ["Other", 15]],
        "SPA": [["Christianity (Catholic)", 96], ["Other", 4]],
    },
    "01900_CE": {
        "GBR": [["Christianity", 60], ["No religion", 20], ["Islam", 2], ["Other", 18]],
        "FRA": [["Christianity (Catholic)", 60], ["No religion", 25], ["Islam", 4], ["Other", 11]],
        "GER": [["Christianity (Protestant)", 35], ["Christianity (Catholic)", 33], ["No religion", 20], ["Other", 12]],
        "RUS": [["Christianity (Orthodox)", 75], ["Islam", 10], ["No religion", 10], ["Other", 5]],
        "OTT": [["Islam", 88], ["Christianity", 8], ["Judaism", 3], ["Other", 1]],
        "USA": [["Christianity (Protestant)", 50], ["Christianity (Catholic)", 24], ["No religion", 10], ["Other", 16]],
        "JPN": [["Shinto", 60], ["Buddhism", 30], ["Other", 10]],
        "ITA": [["Christianity (Catholic)", 90], ["No religion", 5], ["Other", 5]],
        "QIN": [["Chinese folk", 55], ["No religion", 25], ["Buddhism", 15], ["Other", 5]],
        "SPA": [["Christianity (Catholic)", 92], ["Other", 8]],
        "BRA": [["Christianity (Catholic)", 80], ["Christianity (Protestant)", 8], ["Other", 12]],
        "MEX": [["Christianity (Catholic)", 90], ["Other", 10]],
        "IND": [["Hinduism", 80], ["Islam", 14], ["Sikhism", 2], ["Other", 4]],
    },
    "01938_CE": {
        "GER": [["Christianity (Protestant)", 55], ["Christianity (Catholic)", 35], ["No religion", 5], ["Other", 5]],
        "USSR": [["Christianity (Orthodox)", 55], ["Islam", 12], ["No religion (state atheism)", 25], ["Other", 8]],
        "USA": [["Christianity (Protestant)", 55], ["Christianity (Catholic)", 22], ["Other", 23]],
        "GBR": [["Christianity (Anglican)", 65], ["Other", 35]],
        "FRA": [["Christianity (Catholic)", 80], ["No religion", 12], ["Other", 8]],
        "ITA": [["Christianity (Catholic)", 95], ["Other", 5]],
        "JPN": [["Shinto", 70], ["Buddhism", 25], ["Other", 5]],
        "CHN": [["Chinese folk", 60], ["Buddhism", 20], ["No religion", 15], ["Other", 5]],
        "IND": [["Hinduism", 75], ["Islam", 18], ["Other", 7]],
    },
    "01945_CE": {
        "USA": [["Christianity (Protestant)", 55], ["Christianity (Catholic)", 22], ["Judaism", 3], ["Other", 20]],
        "USSR": [["Christianity (Orthodox)", 55], ["Islam", 12], ["No religion (state atheism)", 25], ["Other", 8]],
        "GBR": [["Christianity (Anglican)", 65], ["Other", 35]],
        "FRA": [["Christianity (Catholic)", 82], ["No religion", 10], ["Other", 8]],
        "CHN": [["Chinese folk", 55], ["Buddhism", 20], ["No religion", 20], ["Other", 5]],
        "GER": [["Christianity (Protestant)", 50], ["Christianity (Catholic)", 40], ["Other", 10]],
        "ITA": [["Christianity (Catholic)", 95], ["Other", 5]],
        "JPN": [["Shinto", 70], ["Buddhism", 25], ["Other", 5]],
        "IND": [["Hinduism", 80], ["Islam", 15], ["Other", 5]],
    },
    "01991_CE": {
        "USA": [["Christianity (Protestant)", 52], ["Christianity (Catholic)", 25], ["No religion", 8], ["Other", 15]],
        "RUS": [["Christianity (Orthodox)", 60], ["Islam", 10], ["No religion", 25], ["Other", 5]],
        "CHN": [["Chinese folk", 50], ["Buddhism", 20], ["No religion", 25], ["Other", 5]],
        "GBR": [["Christianity", 55], ["No religion", 20], ["Other", 25]],
        "FRA": [["Christianity (Catholic)", 65], ["No religion", 20], ["Islam", 5], ["Other", 10]],
        "GER": [["Christianity (Protestant)", 38], ["Christianity (Catholic)", 34], ["No religion", 20], ["Other", 8]],
        "JPN": [["Shinto", 55], ["Buddhism", 32], ["Other", 13]],
        "IND": [["Hinduism", 82], ["Islam", 12], ["Sikhism", 2], ["Other", 4]],
        "BRA": [["Christianity (Catholic)", 75], ["Christianity (Protestant)", 15], ["Other", 10]],
        "MEX": [["Christianity (Catholic)", 90], ["Other", 10]],
        "NGA": [["Islam", 50], ["Christianity", 45], ["Traditional", 5]],
        "SAU": [["Islam", 97], ["Other", 3]],
        "IRN": [["Islam (Shia)", 90], ["Islam (Sunni)", 8], ["Other", 2]],
    },
}

# Modern (2020) religion shares per country (top groups, sums ~100).
MODERN_RELIGION = {
    "USA": [["Christianity (Protestant)", 40], ["Christianity (Catholic)", 21], ["No religion", 29], ["Other", 10]],
    "CHN": [["No religion", 52], ["Chinese folk", 22], ["Buddhism", 18], ["Other", 8]],
    "IND": [["Hinduism", 80], ["Islam", 14], ["Christianity", 2], ["Sikhism", 2], ["Other", 2]],
    "IDN": [["Islam", 87], ["Christianity", 10], ["Other", 3]],
    "PAK": [["Islam", 96], ["Hinduism", 2], ["Other", 2]],
    "BRA": [["Christianity (Catholic)", 55], ["Christianity (Protestant)", 25], ["No religion", 10], ["Other", 10]],
    "NGA": [["Islam", 50], ["Christianity", 46], ["Traditional", 4]],
    "BGD": [["Islam", 91], ["Hinduism", 8], ["Other", 1]],
    "RUS": [["Christianity (Orthodox)", 65], ["No religion", 15], ["Islam", 10], ["Other", 10]],
    "MEX": [["Christianity (Catholic)", 78], ["Christianity (Protestant)", 10], ["No religion", 8], ["Other", 4]],
    "JPN": [["Shinto", 45], ["Buddhism", 35], ["No religion", 15], ["Other", 5]],
    "PHL": [["Christianity (Catholic)", 80], ["Christianity (Protestant)", 10], ["Islam", 5], ["Other", 5]],
    "EGY": [["Islam", 90], ["Christianity (Coptic)", 9], ["Other", 1]],
    "ETH": [["Christianity (Orthodox)", 44], ["Islam", 34], ["Christianity (Protestant)", 18], ["Other", 4]],
    "VNM": [["No religion", 70], ["Buddhism", 14], ["Christianity", 8], ["Other", 8]],
    "DEU": [["Christianity", 50], ["No religion", 38], ["Islam", 6], ["Other", 6]],
    "IRN": [["Islam (Shia)", 90], ["Islam (Sunni)", 8], ["Other", 2]],
    "TUR": [["Islam", 90], ["No religion", 5], ["Other", 5]],
    "THA": [["Buddhism", 94], ["Islam", 4], ["Other", 2]],
    "ZAF": [["Christianity", 80], ["Traditional", 10], ["Other", 10]],
    "ITA": [["Christianity (Catholic)", 75], ["No religion", 20], ["Other", 5]],
    "ESP": [["Christianity (Catholic)", 62], ["No religion", 30], ["Other", 8]],
    "FRA": [["Christianity (Catholic)", 48], ["No religion", 35], ["Islam", 7], ["Other", 10]],
    "GBR": [["Christianity", 45], ["No religion", 38], ["Islam", 6], ["Other", 11]],
    "KOR": [["No religion", 55], ["Christianity", 28], ["Buddhism", 15], ["Other", 2]],
    "COL": [["Christianity (Catholic)", 70], ["Christianity (Protestant)", 18], ["No religion", 8], ["Other", 4]],
    "ARG": [["Christianity (Catholic)", 62], ["Christianity (Protestant)", 13], ["No religion", 18], ["Other", 7]],
    "DZA": [["Islam", 99], ["Other", 1]],
    "CAN": [["Christianity", 55], ["No religion", 30], ["Islam", 4], ["Other", 11]],
    "SAU": [["Islam", 97], ["Other", 3]],
    "UKR": [["Christianity (Orthodox)", 55], ["Christianity (Catholic)", 15], ["No religion", 20], ["Other", 10]],
    "MAR": [["Islam", 99], ["Other", 1]],
    "MYT": [["Islam", 97], ["Other", 3]],
    "AUS": [["Christianity", 45], ["No religion", 40], ["Other", 15]],
    "ISR": [["Judaism", 74], ["Islam", 18], ["Christianity", 2], ["Other", 6]],
    "IRQ": [["Islam (Shia)", 60], ["Islam (Sunni)", 32], ["Christianity", 2], ["Other", 6]],
    "LKA": [["Buddhism", 70], ["Hinduism", 13], ["Islam", 10], ["Christianity", 7]],
    "AFG": [["Islam", 99], ["Other", 1]],
    "UZB": [["Islam", 90], ["Other", 10]],
}

# Modern (2020) ethnicity shares per country (top groups, sums ~100).
MODERN_ETHNICITY = {
    "USA": [["White", 60], ["Hispanic", 19], ["Black", 13], ["Asian", 6], ["Other", 2]],
    "CHN": [["Han Chinese", 92], ["Zhuang", 1], ["Other", 7]],
    "IND": [["Indo-Aryan", 72], ["Dravidian", 25], ["Other", 3]],
    "IDN": [["Javanese", 40], ["Sundanese", 15], ["Malay", 10], ["Other", 35]],
    "PAK": [["Punjabi", 45], ["Pashtun", 15], ["Sindhi", 14], ["Other", 26]],
    "BRA": [["White", 43], ["Pardo", 47], ["Black", 8], ["Other", 2]],
    "NGA": [["Hausa", 25], ["Yoruba", 21], ["Igbo", 18], ["Other", 36]],
    "BGD": [["Bengali", 98], ["Other", 2]],
    "RUS": [["Russian", 78], ["Tatar", 4], ["Other", 18]],
    "MEX": [["Mestizo", 62], ["Amerindian", 21], ["White", 10], ["Other", 7]],
    "JPN": [["Japanese", 98], ["Other", 2]],
    "PHL": [["Tagalog", 25], ["Cebuano", 13], ["Ilocano", 9], ["Other", 53]],
    "EGY": [["Egyptian", 99], ["Other", 1]],
    "ETH": [["Oromo", 34], ["Amhara", 27], ["Somali", 6], ["Other", 33]],
    "VNM": [["Kinh (Viet)", 86], ["Other", 14]],
    "DEU": [["German", 82], ["Other", 18]],
    "IRN": [["Persian", 61], ["Azeri", 16], ["Kurd", 10], ["Other", 13]],
    "TUR": [["Turkish", 70], ["Kurd", 19], ["Other", 11]],
    "THA": [["Thai", 85], ["Other", 15]],
    "ZAF": [["Black African", 80], ["White", 9], ["Coloured", 9], ["Other", 2]],
    "ITA": [["Italian", 90], ["Other", 10]],
    "ESP": [["Spanish", 85], ["Other", 15]],
    "FRA": [["French", 85], ["Other", 15]],
    "GBR": [["White British", 74], ["Other White", 6], ["South Asian", 6], ["Other", 14]],
    "KOR": [["Korean", 97], ["Other", 3]],
    "COL": [["Mestizo", 53], ["White", 20], ["Afro-Colombian", 10], ["Other", 17]],
    "ARG": [["European", 70], ["Mestizo", 20], ["Other", 10]],
    "CAN": [["White", 70], ["East Asian", 12], ["South Asian", 5], ["Other", 13]],
    "SAU": [["Arab", 90], ["Other", 10]],
    "UKR": [["Ukrainian", 78], ["Russian", 17], ["Other", 5]],
    "AUS": [["White (European)", 76], ["Asian", 12], ["Indigenous", 3], ["Other", 9]],
    "ISR": [["Jewish", 74], ["Arab", 21], ["Other", 5]],
    "IRQ": [["Arab", 75], ["Kurd", 20], ["Other", 5]],
    "LKA": [["Sinhalese", 75], ["Sri Lankan Tamil", 11], ["Other", 14]],
    "AFG": [["Pashtun", 42], ["Tajik", 27], ["Hazara", 9], ["Other", 22]],
    "UZB": [["Uzbek", 84], ["Other", 16]],
    "MAR": [["Arab", 65], ["Berber", 32], ["Other", 3]],
    "DZA": [["Arab", 70], ["Berber", 28], ["Other", 2]],
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
    era_religion = RELIGION_BY_ERA.get(era_key, {})
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
                "religion_pct": era_religion.get(owner, [["Traditional/Unspecified", 100.0]]),
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
            "religion_pct": MODERN_RELIGION.get(iso, [["Multiple/Unspecified", 100.0]]),
            "ethnicity_pct": MODERN_ETHNICITY.get(iso, [["Multiple/Unspecified", 100.0]]),
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
    (-528, "The Buddha's enlightenment",
     "Siddhartha Gautama attains enlightenment under the bodhi tree; Buddhism \
spreads across India, China, Japan and Southeast Asia."),
    (-331, "Alexander defeats Persia",
     "At Gaugamela, Alexander the Great shatters the Achaemenid Empire, \
carrying Greek culture and cities across Asia."),
    (-27, "Augustus founds the Roman Empire",
     "The Republic gives way to imperial rule under Augustus; two centuries \
of relative peace (the Pax Romana) follow."),
    (312, "Constantine converts; Christianity legalized",
     "After his victory at the Milvian Bridge, Constantine ends persecution \
and Christianity becomes the religion of the Roman world."),
    (325, "Council of Nicaea",
     "Christian bishops convene to define core doctrine; the Nicene Creed \
unifies a faith that will dominate two continents."),
    (1054, "The Great Schism",
     "Roman Catholicism and Eastern Orthodoxy formally split, hardening the \
religious and political boundary between west and east."),
    (1066, "Norman conquest of England",
     "William of Normandy defeats Harold at Hastings, joining England to the \
continental feudal world."),
    (1095, "The First Crusade",
     "Pope Urban II calls for holy war; the resulting kingdom of Jerusalem \
opens two centuries of crusading."),
    (1206, "Genghis Khan unifies the Mongols",
     "Temujin is proclaimed Genghis Khan; the Mongol Empire goes on to forge \
the largest contiguous land empire in history."),
    (1215, "Magna Carta",
     "England's barons force King John to sign limits on royal power — an \
early seed of constitutional rule."),
    (1347, "The Black Death",
     "Plague sweeps from Asia into Europe, killing perhaps half the \
population of the continent within four years."),
    (1453, "Fall of Constantinople",
     "The Ottoman conquest of Constantinople ends the Byzantine Empire and \
accelerates the European Renaissance."),
    (1498, "Vasco da Gama reaches India",
     "The sea route around Africa to India is opened, shifting global trade \
from the Mediterranean to the Atlantic."),
    (1517, "The Protestant Reformation",
     "Martin Luther nails his Ninety-Five Theses; Christendom fractures and \
Europe enters the age of religious wars."),
    (1521, "Fall of the Aztec Empire",
     "Cortés and his allies take Tenochtitlan; within decades Spain dominates \
the Americas from Mexico to Peru."),
    (1533, "Fall of the Inca Empire",
     "Pizarro captures Cusco; the last great indigenous empire of the Andes \
collapses before conquest and disease."),
    (1648, "Peace of Westphalia",
     "The treaties end the Thirty Years' War and establish the modern \
system of sovereign nation-states."),
    (1687, "Newton's Principia",
     "Isaac Newton publishes universal gravitation and the laws of motion, \
the foundation of the Scientific Revolution."),
    (1769, "Watt's steam engine",
     "James Watt patents a vastly improved steam engine, the machine that \
drives the Industrial Revolution."),
    (1804, "World population reaches 1 billion",
     "After two hundred millennia, humanity numbers one billion — the start \
of explosive demographic growth."),
    (1825, "The first public railway",
     "The Stockton and Darlington Railway opens; steam railways will shrink \
continents within a generation."),
    (1848, "Revolutions and the Communist Manifesto",
     "Revolution sweeps Europe; Marx and Engels publish the Communist \
Manifesto, shaping the next century's ideology."),
    (1859, "Origin of Species",
     "Charles Darwin's theory of evolution by natural selection transforms \
the scientific understanding of life."),
    (1869, "Suez Canal opens",
     "The canal cuts Europe's sea route to Asia; European empires tighten \
their grip on Africa and the Indian Ocean."),
    (1918, "Spanish flu pandemic",
     "A brutal influenza pandemic kills tens of millions as World War I \
ends, a global catastrophe quickly forgotten."),
    (1927, "World population reaches 2 billion",
     "Human numbers double in barely a century; cities and industries \
reshape every continent."),
    (1929, "The Great Depression",
     "The Wall Street crash spirals into a worldwide depression, radicalizing \
politics and preparing the ground for war."),
    (1945, "United Nations founded",
     "Fifty nations sign the UN Charter in San Francisco, hoping collective \
security can prevent another world war."),
    (1948, "State of Israel proclaimed",
     "Israel declares independence; the first Arab-Israeli war begins a \
conflict that will define the region for generations."),
    (1957, "Sputnik orbits the Earth",
     "The Soviet Union launches the first artificial satellite; the space \
race — and the missile age — begin."),
    (1960, "World population reaches 3 billion",
     "Mass public health gains drive a global population boom, straining \
food, land and institutions."),
    (1961, "Yuri Gagarin in space; Berlin Wall built",
     "A human enters orbit for the first time, and a wall goes up through \
the heart of Berlin, sealing the Cold War division of Europe."),
    (1974, "World population reaches 4 billion",
     "Growth of more than a billion people in just fourteen years makes \
'sustainable development' a global watchword."),
    (1987, "World population reaches 5 billion",
     "The planet crosses five billion; the UN launches its first \
environmental 'common future' agenda."),
    (1989, "The Berlin Wall falls",
     "Communist regimes collapse across Eastern Europe within months, \
ending the Cold War division of the continent."),
    (1999, "World population reaches 6 billion",
     "Humanity enters the sixth billion; the internet begins to connect \
them all in real time."),
    (2008, "Global financial crisis",
     "The collapse of US housing finance triggers the worst global \
recession since the 1930s."),
    (2011, "Arab Spring",
     "Protest movements topple regimes across North Africa and the Middle \
East, redrawing the region's politics."),
    (2011, "World population reaches 7 billion",
     "Humanity passes seven billion in a world of rising urbanization and \
aging societies."),
    (2015, "Paris Climate Agreement",
     "Nearly every nation commits to limiting global warming, the first \
universal climate accord in history."),
    (2022, "Russia invades Ukraine",
     "A full-scale European war returns, redrawing energy and security \
alignments worldwide."),
    (2022, "World population reaches 8 billion",
     "Humanity reaches eight billion — peak rates are now behind us, with \
global population growth slowing."),
    (2023, "Generative AI goes mainstream",
     "Large language models reach hundreds of millions of users within \
months, the fastest technology adoption in history."),
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
    conn.execute("INSERT INTO meta VALUES ('seed_version', '1.2.0-expanded')")
    conn.commit()
    conn.close()

    print(f"built {OUT}")
    print(f"  events: {len(events)}")
    print(f"  first: {events[0][2]}-{events[0][3]:02d}-{events[0][4]:02d} {events[0][5]}")
    print(f"  last:  {events[-1][2]}-{events[-1][3]:02d}-{events[-1][4]:02d} {events[-1][5]}")


if __name__ == "__main__":
    build()
