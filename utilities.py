import random

def random_french_regiment():
    numbers = list(range(1, 100))
    
    names = [
        "Picardie",
        "Piémont",
        "Navarre",
        "Champagne",
        "Normandie",
        "La Marine",
        "Auvergne",
        "Bourbonnais",
        "Flandre",
        "Artois",
        "Bourbon",
        "Soissonnais",
        "Béarn",
        "Armagnac",
        "Guyenne",
        "La Sarre",
        "Royal Roussillon",
        "Languedoc",
        "Berry",
        "Dauphin",
        "Royal Barrois",
        "Royal Lorraine",
        "Royal Corse",
        "Royal Deux-Ponts",
        "Royal Bavière"
    ]

    number = random.choice(numbers)
    name = random.choice(names)

    return f"{number}e Régiment de {name}"


# Example:
for i in range(10):
    print(random_french_regiment())

print("Great Britian")

def random_british_regiment():
    numbers = list(range(1, 120))

    names = [
        # Royal regiments
        "Royal Scots",
        "Royal Welsh Fusiliers",
        "Royal Irish",
        "King's Own",
        "Queen's Own",
        "Royal Americans",
        "Royal Highlanders",
        
        # Famous British regiments in the Seven Years' War era
        "Barrell's",
        "Montgomerie's Highlanders",
        "Fraser's Highlanders",
        "Campbell's Highlanders",
        "Amherst's",
        "Wolfe's",
        "Ligonier's",
        "Hale's",
        "Howard's",
        "Webb's",
        "Pulteney's",
        "Bligh's",
        "Earl of Loudoun's",
        "Lord George Sackville's",
        "Earl of Abercorn's",
        "Lord Charles Hay's",
        "Earl of Home's",
        "Earl of Rothes's",
        "Lord John Murray's",
        "Montagu's",
        "Anstruther's",
        "Dury's",
        
        # Infantry style names
        "Highlanders",
        "Foot Guards",
        "Light Infantry",
        "Grenadiers",
        "Fusiliers",
        "Scottish Foot",
        "Irish Foot",
        "English Foot",
        "American Provincials",
        
        # More colonel-style names
        "Bragg's",
        "Dalrymple's",
        "Murray's",
        "Forbes's",
        "Kennedy's",
        "Stanley's",
        "Egerton's",
        "Barrington's",
        "Hotham's",
        "Cholmondeley's",
        "Cornwallis's",
        "Elliot's",
        "Carleton's",
        "Grant's",
        "Mackenzie's",
        "Stuart's",
        "Douglas's",
        "Hamilton's",
        "Gordon's",
        "Maclean's",
        "Sutherland's",
        "Argyll's",
        "Atholl's",
        "Bute's",
        "Abercromby's"
    ]

    number = random.choice(numbers)
    name = random.choice(names)

    return f"{number}th Regiment of Foot ({name})"


# Example:
for i in range(15):
    print(random_british_regiment())

print("Spain")

def random_spanish_regiment():
    numbers = list(range(1, 100))

    names = [
        # Famous Spanish infantry regiments
        "España",
        "Castilla",
        "Soria",
        "Saboya",
        "Asturias",
        "Aragón",
        "Galicia",
        "Burgos",
        "León",
        "Toledo",
        "Murcia",
        "Córdoba",
        "Granada",
        "Lombardía",
        "Nápoles",
        "Milán",
        "Guadalajara",
        "Princesa",
        "Corona",
        "Real América",
        
        # Royal / Bourbon style
        "Real de España",
        "Real de la Reina",
        "Rey",
        "Infante",
        "Príncipe",
        "Reina",
        "Corona Real",
        "Guardias Españolas",
        "Guardias Valonas",
        
        # Regional names
        "Navarra",
        "Valencia",
        "Cataluña",
        "Extremadura",
        "Andalucía",
        "Canarias",
        "Mallorca",
        "Asturias",
        "Galicia",
        "Vizcaya",
        "Badajoz",
        "Zamora",
        "África",
        
        # Historical-style colonel names
        "López",
        "García",
        "Fernández",
        "Mendoza",
        "Pacheco",
        "Alvarado",
        "Montesinos",
        "Villalba",
        "Cárdenas",
        "Velasco",
        "Figueroa",
        "Rojas",
        "Molina",
        "Carrillo",
        "Hernández",
        "Zúñiga",
        "Castro",
        "Guzmán",
        "Manrique",
        "Osorio",
        
        # Foreign service / elite units
        "Hibernia",
        "Ultonia",
        "Irlanda",
        "Flandes",
        "Brabante",
        "Saboya",
        "Borgoña",
        "Italia",
        "Suiza",
        "Real Extranjero"
    ]

    number = random.choice(numbers)
    name = random.choice(names)

    return f"{number}º Regimiento de Infantería de {name}"


# Example:
for i in range(15):
    print(random_spanish_regiment())

print("Russia")

def random_russian_regiment():
    numbers = list(range(1, 100))

    names = [
        # Famous Russian infantry regiments
        "Moscow",
        "Kiev",
        "Novgorod",
        "Vladimir",
        "Smolensk",
        "Kazan",
        "Astrakhan",
        "Pskov",
        "Narva",
        "Vyborg",
        "Riga",
        "Tobolsk",
        "Siberia",
        "Voronezh",
        "Perm",
        "Azov",
        "Arkhangelsk",
        "Belgorod",
        "Yaroslavl",
        "Nizhny Novgorod",
        
        # Royal / elite names
        "Preobrazhensky",
        "Semyonovsky",
        "Izmailovsky",
        "Life Grenadier",
        "Moscow Grenadier",
        "Kiev Grenadier",
        "Saint Petersburg",
        "Imperial Guard",
        
        # More historical Russian-style names
        "Novgorod Musketeer",
        "Narva Musketeer",
        "Vladimir Musketeer",
        "Tula Musketeer",
        "Rostov Musketeer",
        "Kexholm",
        "Murom",
        "Suzdal",
        "Ryazan",
        "Vyatka",
        "Tver",
        "Kursk",
        "Oryol",
        "Tambov",
        "Kostroma",
        
        # Colonel-style names
        "Bestuzhev's",
        "Saltykov's",
        "Rumyansev's",
        "Lopukhin's",
        "Golitsyn's",
        "Dolgorukov's",
        "Panin's",
        "Vorontsov's",
        "Repnin's",
        "Golicyn's",
        "Shuvalov's",
        "Chernyshov's",
        "Apraksin's",
        "Lacy's",
        "Browne's",
        
        # Foreign / recruited units
        "Livonian",
        "Estonian",
        "Finnish",
        "Ingria",
        "Lifland",
        "Kurland",
        "Baltic",
        "German Legion"
    ]

    number = random.choice(numbers)
    name = random.choice(names)

    return f"{number}th Russian Regiment of {name}"


# Example:
for i in range(15):
    print(random_russian_regiment())

    import random
# Comanche Settlements
def random_comanche_settlement():
    names = [
        "Buffalo River Camp",
        "Red Prairie Camp",
        "Antelope Hills Camp",
        "Palo Duro Camp",
        "Upper Arkansas Camp",
        "Red River Camp",
        "Great Plains Camp",
        "Horse Creek Camp",
        "Tall Grass Camp",
        "Thunder Valley Camp",
        "Painted Hills Camp",
        "Buffalo Trail Camp",
        "Medicine River Camp",
        "Windy Plains Camp",
        "Eagle Ridge Camp",
        "Cottonwood Camp",
        "Running Horse Camp",
        "Stone Valley Camp",
        "Sunset Plains Camp",
        "Wild Buffalo Camp",
        "Big River Camp",
        "Prairie Flower Camp",
        "Wolf Creek Camp",
        "Mountain Shadow Camp",
        "Golden Grass Camp"
    ]

    return random.choice(names)

# Example
print(random_comanche_settlement())


def random_comanche_settlement():
    names = [
        "Quahadi Camp", "Nokoni Camp", "Penateka Camp", "Kotsoteka Camp",
        "Yamparika Camp", "Tenewa Camp", "Buffalo Springs", "Red River Camp",
        "Palo Duro Camp", "Llano Estacado Camp", "Medicine Mound",
        "Wichita Camp", "Antelope Hills", "Elk Creek Camp",
        "Canyon Camp", "Cimarron Camp", "Prairie Dog Town"
    ]
    return random.choice(names)
# American
import random

print("United States")

def random_american_regiment():
    numbers = list(range(1, 100))

    names = [
        # States / colonies
        "Virginia",
        "Massachusetts",
        "Pennsylvania",
        "New York",
        "Connecticut",
        "Rhode Island",
        "New Jersey",
        "Delaware",
        "Maryland",
        "North Carolina",
        "South Carolina",
        "Georgia",
        "New Hampshire",
        "Vermont",
        
        # Regional / line names
        "Continental",
        "Eastern",
        "Western",
        "Northern",
        "Southern",
        "Frontier",
        "Rifle",
        "Light Infantry",
        "Grenadier",
        "Rangers",
        
        # Famous Revolutionary commanders
        "Washington's",
        "Greene's",
        "Morgan's",
        "Wayne's",
        "Lee's",
        "Arnold's",
        "Putnam's",
        "Stark's",
        "Knox's",
        "Sullivan's",
        "Lafayette's",
        "Pulaski's",
        "Muhlenberg's",
        "Poor's",
        "Gist's",
        "Maxwell's",
        "Clinton's",
        "Livingston's",
        "Smallwood's",
        "Hand's",
        
        # Historic Continental Army style
        "Flying Camp",
        "Pennsylvania Line",
        "Virginia Line",
        "Maryland Line",
        "New England Line",
        "Carolina Line",
        "Jersey Line",
        "Delaware Line",
        "Connecticut Line",
        "New York Line",
        
        # Militia style
        "County Militia",
        "State Militia",
        "Volunteer Militia",
        "Minute Men",
        "Riflemen",
        "Militia Volunteers",
        "Mounted Rangers",
        "Frontier Guards",
        
        # More period-style names
        "Liberty",
        "Independence",
        "Freedom",
        "Patriot",
        "Republic",
        "Constitution",
        "Union",
        "American Legion",
        "Commonwealth",
        "Revolutionary Guards"
    ]

    number = random.choice(numbers)
    name = random.choice(names)

    return f"{number}th American Regiment of {name}"


# Example:
for i in range(15):
    print(random_american_regiment())


def random_american_regiment():
    numbers = list(range(1, 30))
    names = [
        "Continental Line", "Minutemen", "Virginia Militia", "Massachusetts Militia",
        "Pennsylvania Rifles", "Maryland Line", "Connecticut Line", "New Jersey Line",
        "Delaware Regiment", "Rhode Island Regiment", "New Hampshire Militia",
        "Green Mountain Boys", "Morgan's Rifles", "Knox's Artillery",
        "Light Dragoons", "Continental Marines", "Washington's Guard"
    ]
    number = random.choice(numbers)
    name = random.choice(names)
    return f"{number}th {name}"

# Cree Settlements
def random_cree_settlement():
    names = [
        "Mistawasis", "Pimicikamak", "Wapaskwaw", "Maskwa Sipi",
        "Kawasiskwaw", "Wapamew", "Sakawiyiniwak", "Nistawayaw",
        "Paskwaw", "Kiskisink", "Waskaganish", "Nemaska",
        "Mistissini", "Waswanipi", "Pikogan", "Attawapiskat"
    ]
    return random.choice(names)

# Dakota Settlements
def random_dakota_settlement():
    names = [
        "Mdewakantonwan", "Wahpekute", "Sisseton", "Wahpeton",
        "Yanktonai", "Ihanktonwan", "Titonwan", "Sicangu",
        "Oglala", "Hunkpapa", "Minneconjou", "Sans Arc",
        "Two Kettles", "Blackfoot Sioux", "Bdewakantunwan", "Isanti"
    ]
    return random.choice(names)

# Danish Regiments
def random_danish_regiment():
    numbers = list(range(1, 30))
    names = [
        "Sjaellandske", "Jyske", "Fynske", "Norske", "Holstenske",
        "Oldenborgske", "Dronningens", "Kongens", "Prins Frederiks",
        "Slesvigske", "Danske Livregiment", "Grenader",
        "Sonderjyske", "Bornholmske", "Falsterske"
    ]
    number = random.choice(numbers)
    name = random.choice(names)
    return f"{number}. {name} Regiment"

english_first_names = ["William", "John", "Thomas", "Edward", "George", "Henry", "Charles", "James", "Samuel", "Edmund"]

english_last_names = ["Fairfax", "Wentworth", "Harrington", "Cavendish", "Fitzwilliam", "Granville", "Somerset", "Pembroke", "Talbot", "Wellesley"]

french_first_names = ["Jean", "Pierre", "Louis", "Charles", "François", "Jacques", "Antoine", "Joseph", "Henri", "Étienne"]

french_last_names = ["Dubois", "Moreau", "Lefèvre", "Bernard", "Rousseau", "Laurent", "Mercier", "Fontaine", "Beaumont", "Dumont"]

spanish_first_names = ["Juan", "Carlos", "Francisco", "José", "Miguel", "Antonio", "Pedro", "Diego", "Fernando", "Javier"]

spanish_last_names = ["García", "Fernández", "López", "Martínez", "Sánchez", "González", "Rodríguez", "Navarro", "Castillo", "Mendoza"]

american_first_names = ["John", "William", "Thomas", "Samuel", "Benjamin", "Jonathan", "James", "Joseph", "Nathaniel", "Isaac"]

american_last_names = ["Adams", "Washington", "Jefferson", "Franklin", "Hancock", "Carroll", "Livingston", "Winthrop", "Madison", "Paine"]

mexican_first_names = ["Juan", "José", "Miguel", "Francisco", "Antonio", "Pedro", "Carlos", "Manuel", "Diego", "Fernando"]

mexican_last_names = ["García", "Hernández", "López", "Martínez", "González", "Rodríguez", "Ramírez", "Sánchez", "Mendoza", "Castillo"]

haitian_first_names = ["Jean", "Pierre", "François", "Louis", "Antoine", "Jacques", "Joseph", "Charles", "Henri", "Étienne"]

haitian_last_names = ["Dubois", "Moreau", "Lafleur", "Beauvais", "Dumont", "Fontaine", "Bernard", "Lacroix", "Delorme", "Rousseau"]

danish_first_names = ["Jens", "Hans", "Christian", "Frederik", "Niels", "Anders", "Lars", "Peter", "Jørgen", "Mads"]

danish_last_names = ["Jensen", "Hansen", "Nielsen", "Andersen", "Pedersen", "Christensen", "Larsen", "Madsen", "Rasmussen", "Sørensen"]

russian_first_names = ["Ivan", "Peter", "Alexander", "Dmitry", "Mikhail", "Nikolai", "Vasily", "Fyodor", "Alexei", "Grigory"]

russian_last_names = ["Ivanov", "Petrov", "Sokolov", "Smirnov", "Volkov", "Orlov", "Kuznetsov", "Popov", "Morozov", "Romanov"]


# Governor name generation per faction
_GOVERNOR_NAMES = {
    "Great Britain": (english_first_names, english_last_names),
    "France": (french_first_names, french_last_names),
    "Spain": (spanish_first_names, spanish_last_names),
    "United States": (american_first_names, american_last_names),
    "Mexico": (mexican_first_names, mexican_last_names),
    "Haiti": (haitian_first_names, haitian_last_names),
    "Denmark": (danish_first_names, danish_last_names),
    "Russia": (russian_first_names, russian_last_names),
    "Texas": (american_first_names, american_last_names),
    "Confederate States": (american_first_names, american_last_names),
    "Rupert's Land": (english_first_names, english_last_names),
    "Canada": (english_first_names, english_last_names),
}

# Native chief names
_CHIEF_NAMES = {
    "Iroquois": ["Hiawatha", "Deganawida", "Thayendanegea", "Cornplanter", "Red Jacket", "Handsome Lake", "Canasatego", "Skenandoa", "Pontiac", "Logan"],
    "Wabanaki": ["Madockawando", "Moxus", "Bomazeen", "Atecouando", "Assacumbuit", "Nescambious", "Taxous", "Wowurna", "Abenaki", "Bashaba"],
    "Comanche": ["Quanah", "Iron Jacket", "Buffalo Hump", "Ten Bears", "Peta Nocona", "White Eagle", "Mow-way", "Horseback", "Isa-tai", "Tosawi"],
    "Cree": ["Poundmaker", "Big Bear", "Piapot", "Maskepetoon", "Sweetgrass", "Ahtahkakoop", "Mistawasis", "Star Blanket", "Thunderchild", "Little Pine"],
    "Dakota": ["Sitting Bull", "Red Cloud", "Crazy Horse", "Spotted Tail", "Rain-in-the-Face", "Gall", "Touch the Clouds", "American Horse", "Little Crow", "Wapasha"],
}

def random_chief_name(faction):
    if faction in _CHIEF_NAMES:
        return random.choice(_CHIEF_NAMES[faction])
    return "Chief"

def random_governor_name(faction):
    if faction in _GOVERNOR_NAMES:
        firsts, lasts = _GOVERNOR_NAMES[faction]
        return f"{random.choice(firsts)} {random.choice(lasts)}"
    return "Unknown Governor"
