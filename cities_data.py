"""Starting city / settlement definitions for Colonial Simulator.

Extracted from index.py to keep the main file smaller. Dependencies (coordinate scale,
strategy list, city material list) are passed in to avoid circular imports.

build_cities(coord_scale, strategies, city_materials) -> (cities, founded_cities)

`cities` are the settlements present at game start. `founded_cities` appear later at
their historical `_founded_year`. NOTE: as in the original, make_city/make_village entries
are pre-scaled (they carry "_scaled": True); the inline fort/camp dicts are left UNSCALED
and are scaled by index.py's downstream pass, and founded-city coords are scaled by index.py.
"""
import random


def build_cities(coord_scale, strategies, city_materials):
    def make_city(name, x, y, color, owner, capital):
        x *= coord_scale; y *= coord_scale
        return {"name": name, "x": x, "y": y, "color": color, "troops": 200 if capital else 100,
                "strategy": random.choice(strategies), "owner": owner, "sovereign": owner,
                "_original_sovereign": owner, "occupier": None, "material": random.choice(city_materials),
                "is_capital": capital, "tier": 1, "is_village": False, "is_fort": False,
                "spawn_cd": 0, "buildings": [], "construction": None, "_scaled": True}

    def make_village(name, x, y, color, owner):
        x *= coord_scale; y *= coord_scale
        return {"name": name, "x": x, "y": y, "color": color, "troops": 50,
                "strategy": random.choice(strategies), "owner": owner, "sovereign": owner,
                "_original_sovereign": owner, "occupier": None, "material": random.choice(["Lumber", "Hide"]),
                "is_capital": False, "tier": 0, "is_village": True, "is_fort": False,
                "spawn_cd": 0, "buildings": [], "construction": None, "_scaled": True}

    cities = [
        make_city("Mexico City",171,340,(255,255,0),"Spain",True), make_city("Havana",260,318,(255,255,0),"Spain",False),
        make_city("Oaxaca",187,354,(255,255,0),"Spain",False),
        make_city("Merida",220,334,(255,255,0),"Spain",False), make_city("Albuquerque",145,258,(255,255,0),"Spain",False),
        make_city("San Agustin",256,284,(255,255,0),"Spain",False), make_city("Monterey",80,234,(255,255,0),"Spain",False), make_city("San Diego",95,258,(255,255,0),"Spain",False), make_city("Chihuahua",145,290,(255,255,0),"Spain",False),
        make_city("Caracas",343,370,(255,255,0),"Spain",False),
        make_city("Barranquilla",301,371,(255,255,0),"Spain",False),
        make_city("Santo Domingo",317,328,(255,255,0),"Spain",False),
        make_city("Quebec",280,180,(0,100,255),"France",True), make_city("Montreal",272,189,(0,100,255),"France",False), make_city("New Orleans",217,286,(0,100,255),"France",False),
        make_city("Saint-Louis",212,242,(0,100,255),"France",False), make_city("Fort Detroit",241,218,(0,100,255),"France",False),
        make_city("Port-au-Prince",306,331,(0,100,255),"France",False), make_city("Plaisance",332,161,(0,100,255),"France",False), make_village("Tadoussac",283,171,(0,100,255),"France"),
        {"name":"Saint-Pierre","x":376,"y":350,"color":(0,100,255),"troops":50,"strategy":random.choice(strategies),"owner":"France","sovereign":"France","occupier":None,"material":"Gold","is_capital":False,"tier":0,"is_village":True,"is_fort":False,"is_camp":False,"spawn_cd":0,"buildings":[],"construction":None},
        make_city("Boston",287,208,(255,0,0),"Great Britain",True), make_city("New York",280,218,(255,0,0),"Great Britain",False), make_city("Halifax",309,186,(255,0,0),"Great Britain",False),
        make_city("Charleston",262,264,(255,0,0),"Great Britain",False), make_village("Williamsburg",272,239,(255,0,0),"Great Britain"), make_city("Richmond",267,237,(250,0,0),"Great Britain",False),
        make_city("Kingston",285,338,(255,0,0),"Great Britain",False), make_city("York Factory",202,143,(200,120,60),"Rupert's Land",True), make_city("Nassau",276,302,(255,0,0),"Great Britain",False),
        make_city("Belize Town",228,350,(255,0,0),"Great Britain",False),
        make_city("Guatemala",216,364,(255,255,0),"Spain",False),
        make_city("San Jose",249,382,(255,255,0),"Spain",False),
        {"name":"George Town","x":263,"y":336,"color":(255,0,0),"troops":50,"strategy":random.choice(strategies),"owner":"Great Britain","sovereign":"Great Britain","occupier":None,"material":"Gold","is_capital":False,"tier":0,"is_village":True,"is_fort":False,"is_camp":False,"spawn_cd":0,"buildings":[],"construction":None},
        make_city("Sitka",84,120,(0,120,0),"Russia",True), make_city("Kodiak",52,91,(0,120,0),"Russia",False),
        make_city("Godthaab",284,81,(0,255,0),"Denmark",True), make_city("Reykjavik",317,46,(0,255,0),"Denmark",False),
        make_village("Onondaga",265,211,(150,50,200),"Iroquois"), make_village("Norridgewock",287,192,(128,0,0),"Wabanaki"),
        {"name":"Comancheria","x":161,"y":260,"color":(180,120,40),"troops":30,"strategy":random.choice(strategies),"owner":"Comanche","sovereign":"Comanche","occupier":None,"material":"Hide","is_capital":False,"tier":0,"is_village":False,"is_fort":True,"is_camp":True,"spawn_cd":0},
        make_city("San Antonio",174,293,(255,255,0),"Spain",False),
        {"name":"Quahadi Camp","x":165,"y":265,"color":(180,120,40),"troops":30,"strategy":random.choice(strategies),"owner":"Comanche","sovereign":"Comanche","occupier":None,"material":"Hide","is_capital":False,"tier":0,"is_village":False,"is_fort":True,"is_camp":True,"spawn_cd":0},
        {"name":"Penateka Camp","x":175,"y":276,"color":(180,120,40),"troops":30,"strategy":random.choice(strategies),"owner":"Comanche","sovereign":"Comanche","occupier":None,"material":"Hide","is_capital":False,"tier":0,"is_village":False,"is_fort":True,"is_camp":True,"spawn_cd":0},
        make_village("Moosonee",242,176,(60,180,130),"Cree"), make_village("Opaskwayak",174,163,(60,180,130),"Cree"),
        make_village("Chisasibi",246,163,(60,180,130),"Cree"), make_village("Waskahigan",133,157,(60,180,130),"Cree"),
        make_village("Mdewakantonwan",200,210,(180,160,80),"Dakota"), make_village("Wahpekute",204,214,(180,160,80),"Dakota"),
        {"name":"Oglala Camp","x":163,"y":215,"color":(180,160,80),"troops":30,"strategy":random.choice(strategies),"owner":"Dakota","sovereign":"Dakota","occupier":None,"material":"Hide","is_capital":False,"tier":0,"is_village":False,"is_fort":True,"is_camp":True,"spawn_cd":0},
    ]

    # Cities founded at specific dates (hidden until their year). Left unscaled here; index.py scales them.
    founded_cities = [
        {"name":"Washington","x":267,"y":229,"color":(100,180,255),"troops":100,"strategy":"Balanced","owner":"United States","sovereign":"United States","_original_sovereign":"United States","occupier":None,"material":"Lumber","is_capital":False,"tier":1,"is_village":False,"is_fort":False,"spawn_cd":0,"buildings":[],"construction":None,"_regen_cooldown":0,"_founded_year":1790},
        {"name":"Austin","x":178,"y":287,"color":(100,180,255),"troops":100,"strategy":"Balanced","owner":"United States","sovereign":"United States","occupier":None,"material":"Lumber","is_capital":False,"tier":0,"is_village":False,"is_fort":False,"spawn_cd":0,"buildings":[],"construction":None,"_regen_cooldown":0,"_founded_year":1836},
        {"name":"Victoria","x":95,  "y":170,"color":(255,0,0),"troops":100,"strategy":"Balanced","owner":"Great Britain","sovereign":"Great Britain","occupier":None,"material":"Lumber","is_capital":False,"tier":0,"is_village":False,"is_fort":False,"spawn_cd":0,"buildings":[],"construction":None,"_regen_cooldown":0,"_founded_year":1843},
        {"name":"Ottawa","x":268,"y":197,"color":(255,0,0),"troops":100,"strategy":"Balanced","owner":"Great Britain","sovereign":"Great Britain","occupier":None,"material":"Lumber","is_capital":False,"tier":0,"is_village":False,"is_fort":False,"spawn_cd":0,"buildings":[],"construction":None,"_regen_cooldown":0,"_founded_year":1855},
        {"name":"Chicago","x":223,"y":223,"color":(100,180,255),"troops":100,"strategy":"Balanced","owner":"United States","sovereign":"United States","_original_sovereign":"United States","occupier":None,"material":"Lumber","is_capital":False,"tier":1,"is_village":False,"is_fort":False,"spawn_cd":0,"buildings":[],"construction":None,"_regen_cooldown":0,"_founded_year":1833},
        {"name":"York","x":254,"y":206,"color":(255,0,0),"troops":100,"strategy":"Balanced","owner":"Great Britain","sovereign":"Great Britain","_original_sovereign":"Great Britain","occupier":None,"material":"Lumber","is_capital":False,"tier":0,"is_village":False,"is_fort":False,"spawn_cd":0,"buildings":[],"construction":None,"_regen_cooldown":0,"_founded_year":1787},
    ]
    return cities, founded_cities
