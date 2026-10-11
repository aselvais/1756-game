"""Throwaway analysis: cross-reference images/scenarios/1756map.png (colored by
owner nation) against the game's city coordinates and the overall landmass.

Reports, per nation hex color:
  - which named game cities fall inside that nation's territory on the map
  - that nation's share of all colored (owned) land pixels

City coords are the game's base-layout values (COORD_SCALE = 3), so native pixel
= coord * 3 on the MAP_W x MAP_H canvas; we rescale to the 1756 PNG's own size.
"""

from PIL import Image
from collections import defaultdict

MAP_W = 533 * 3                      # 1599
MAP_H = int(533 * (7717 / 5595)) * 3  # 2205

NATIONS = {
    "Russia":          (0x09, 0x61, 0x1c),
    "Rupert's Land":   (0x73, 0x00, 0x00),
    "Denmark":         (0x1a, 0xbc, 0x69),
    "France":          (0x28, 0x45, 0xd5),
    "Louisiana":       (0x83, 0x8c, 0xbc),
    "Wabanaki":        (0xd0, 0x00, 0x63),
    "Great Britain":   (0xff, 0x00, 0x00),
    "Iroquois":        (0xae, 0x1f, 0xdc),
    "New Spain":       (0xf0, 0xf3, 0x91),
    "Spain":           (0xfe, 0xff, 0x00),
    "Cherokee":        (0x14, 0x59, 0x54),
    "Saint-Domingue":  (0x33, 0x39, 0x58),
    "New Granada":     (0x96, 0x97, 0x00),
    "Netherlands":     (0xff, 0x79, 0x00),
    "Peru":            (0x43, 0x43, 0x00),
    "Rio de la Plata": (0x95, 0x95, 0x4b),
    "Portugal":        (0xb4, 0x00, 0xff),
}

# (name, coord_x, coord_y, kind) — kind just for display.
CITIES = [
    ("Mexico City",171,340,"city"),("Havana",260,318,"city"),("Oaxaca",187,354,"city"),
    ("Merida",220,334,"city"),("Albuquerque",145,258,"city"),("San Agustin",256,284,"city"),
    ("Monterey",82,233,"city"),("San Diego",95,258,"city"),("Chihuahua",145,290,"city"),
    ("Caracas",343,370,"city"),("Barranquilla",301,371,"city"),("Santo Domingo",317,328,"city"),
    ("Quebec",280,180,"city"),("Montreal",272,189,"city"),("New Orleans",217,286,"city"),
    ("Mobile",228,283,"city"),("Saint-Louis",212,242,"city"),("Fort Detroit",241,218,"city"),
    ("Port-au-Prince",306,331,"city"),("Plaisance",332,161,"city"),("Boston",287,208,"city"),
    ("New York",280,218,"city"),("Halifax",309,186,"city"),("Charleston",262,264,"city"),
    ("Richmond",267,237,"city"),("Kingston",285,338,"city"),("York Factory",202,143,"city"),
    ("Nassau",276,302,"city"),("Belize Town",228,350,"city"),("Guatemala",216,364,"city"),
    ("San Jose",249,382,"city"),("Sitka",84,120,"city"),("Kodiak",52,91,"city"),
    ("Godthaab",284,81,"city"),("Reykjavik",317,46,"city"),("San Antonio",174,293,"city"),
    ("Panama",270,386,"city"),("Bogota",308,398,"city"),("Quito",282,428,"city"),
    ("Lima",290,492,"city"),("Cusco",317,499,"city"),("La Paz",338,512,"city"),
    ("Guayaquil",273,440,"city"),("Medellin",294,394,"city"),("Santiago",333,607,"city"),
    ("Montevideo",395,607,"city"),("Buenos Aires",391,613,"city"),("Asuncion",395,559,"city"),
    ("Salvador",497,497,"city"),("Rio de Janeiro",470,550,"city"),("Sao Paulo",447,556,"city"),
    ("Cayenne",424,399,"city"),("Paramaribo",410,391,"city"),
    # villages
    ("Tadoussac",283,171,"village"),("Williamsburg",272,239,"village"),("Onondaga",265,211,"village"),
    ("Norridgewock",287,192,"village"),("Chota",250,253,"village"),("Moosonee",242,176,"village"),
    ("Opaskwayak",174,163,"village"),("Chisasibi",246,163,"village"),("Waskahigan",133,157,"village"),
    ("Mdewakantonwan",200,210,"village"),("Wahpekute",204,214,"village"),
    ("Saint-Pierre",376,350,"village"),("George Town",263,336,"village"),
    # mobile camps
    ("Comancheria",161,260,"camp"),("Quahadi Camp",165,265,"camp"),("Penateka Camp",175,276,"camp"),
    ("Oglala Camp",163,215,"camp"),
]

COORD_SCALE = 3.0
TOL = 60  # sum-of-abs channel distance threshold for a confident match

def nearest(rgb):
    best, bd = None, 10**9
    for name, c in NATIONS.items():
        d = abs(rgb[0]-c[0]) + abs(rgb[1]-c[1]) + abs(rgb[2]-c[2])
        if d < bd:
            bd, best = d, name
    return (best, bd)

def sample(px, pw, ph, nx, ny):
    """Sample the dominant nation color in a small neighborhood around a native
    coordinate, voting so a pixel on a province border doesn't mislead."""
    sx = nx / MAP_W * pw
    sy = ny / MAP_H * ph
    votes = defaultdict(int)
    for dx in range(-4, 5, 2):
        for dy in range(-4, 5, 2):
            x = min(max(int(sx+dx), 0), pw-1)
            y = min(max(int(sy+dy), 0), ph-1)
            r, g, b = px[x, y][:3]
            name, d = nearest((r, g, b))
            if d <= TOL:
                votes[name] += 1
    if not votes:
        return None
    return max(votes, key=votes.get)

def main():
    img = Image.open("images/scenarios/1756map.png").convert("RGB")
    pw, ph = img.size
    px = img.load()
    print(f"1756map.png size: {pw}x{ph}  (game canvas {MAP_W}x{MAP_H})\n")

    # --- cities grouped by the nation whose color they sit on ---
    by_nation = defaultdict(list)
    unowned = []
    for name, cx, cy, kind in CITIES:
        nx, ny = cx * COORD_SCALE, cy * COORD_SCALE
        owner = sample(px, pw, ph, nx, ny)
        label = name if kind == "city" else f"{name} ({kind})"
        if owner is None:
            unowned.append(label)
        else:
            by_nation[owner].append(label)

    print("=== TERRITORIES BY NATION (named settlements inside each nation's land) ===\n")
    for nation in NATIONS:
        places = by_nation.get(nation, [])
        if places:
            print(f"{nation}:")
            for p in places:
                print(f"    - {p}")
            print()
    if unowned:
        print("Unclaimed / gray land (no nation color under the point):")
        for p in unowned:
            print(f"    - {p}")
        print()

    # --- overall land coverage share ---
    print("=== LAND COVERAGE (share of colored/owned pixels) ===\n")
    counts = defaultdict(int)
    step = 3  # sample every 3rd pixel for speed
    for y in range(0, ph, step):
        for x in range(0, pw, step):
            r, g, b = px[x, y][:3]
            if r > 245 and g > 245 and b > 245:
                continue  # white ocean/background
            name, d = nearest((r, g, b))
            if d <= TOL:
                counts[name] += 1
    total = sum(counts.values()) or 1
    for nation in sorted(counts, key=counts.get, reverse=True):
        pct = counts[nation] / total * 100
        print(f"    {nation:16s} {pct:5.1f}%")

if __name__ == "__main__":
    main()
