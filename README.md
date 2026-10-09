# Colonial Simulator

A pygame grand strategy simulation of colonial and nineteenth-century America. The map covers North and South America. You choose a historical start date, then run cities, armies, ships, trade, and diplomacy as the calendar moves through real events.

The window title is **Colonial Simulator**. Start the game from `index.py`.

## Scenarios

From the main menu, choose **Play**, then a start date:

| Year | Scenario |
| --- | --- |
| 1754 | Seven Years' War |
| 1790 | Continental Wars |
| 1809 | War of 1812 |
| 1835 | Mexican-American War |
| 1858 | American Civil War |

Scroll the scenario row if a card is off screen.

Colonial powers at the start of a game include Great Britain, France, Spain, Russia, and Denmark, along with the Iroquois, Wabanaki, Comanche, Cree, and Dakota. Later dates bring in the United States, Mexico, Haiti, Texas, the Confederate States, and Canada. Pirates appear in the Caribbean until 1830. The **Codex** on the main menu shows each nation's flags and unit art.

Cities produce lumber, hide, iron, coal, gold, and food. You raise infantry and cavalry, send merchants and settlers, sail ships, build improvements, and pass laws. Wars, treaties, and alliances are tracked on the map. Historical dates (the American Revolution, independence movements, and later wars) fire as the year advances. Unit sprites change around 1800 and again around 1850.

## Requirements

- Python 3.10 or newer
- [pygame-ce](https://pypi.org/project/pygame-ce/) (imported in code as `pygame`)
- [Pillow](https://pypi.org/project/Pillow/)
- [NumPy](https://pypi.org/project/numpy/)

Pillow and NumPy build the territory overlay from `images/regionmap.png` and decode animated GIF frames such as explosions. A desktop session is required. The game opens a fullscreen window.

## Installation

Clone or download this repository and work from the project directory. The virtual environment (`.venv`) keeps pygame-ce, Pillow, and NumPy off the rest of the system. Create it once. After that, activate it whenever you run the game.

`requirements.txt` installs those three packages. pygame-ce and pygame both provide the `pygame` module, so install only one of them. If `pygame-ce` fails to build, install `pygame` in the same activated environment:

```bash
python -m pip install pygame Pillow numpy
```

Leave any environment with `deactivate`.

### macOS

Python 3.10 or newer must be on your `PATH`. The installer from [python.org](https://www.python.org/downloads/) or Homebrew (`brew install python`) both work. Check the version:

```bash
python3 --version
```

Create the environment, activate it, and install the packages:

```bash
cd 1756-game
python3 -m venv .venv
source .venv/bin/activate
python -m pip install --upgrade pip
python -m pip install -r requirements.txt
```

The next time you open a terminal, only the activate step is required:

```bash
cd 1756-game
source .venv/bin/activate
```

### Linux

Debian, Ubuntu, and related distributions need the venv module installed with Python:

```bash
sudo apt update
sudo apt install python3 python3-pip python3-venv
python3 --version
```

Fedora and RHEL-family systems:

```bash
sudo dnf install python3 python3-pip
python3 --version
```

Then create and activate the environment:

```bash
cd 1756-game
python3 -m venv .venv
source .venv/bin/activate
python -m pip install --upgrade pip
python -m pip install -r requirements.txt
```

Later sessions only need activation:

```bash
cd 1756-game
source .venv/bin/activate
```

### Windows

Install Python 3.10 or newer from [python.org](https://www.python.org/downloads/) and enable **Add python.exe to PATH** in the installer. Confirm it in a new terminal:

```bat
py -3 --version
```

**Command Prompt**

```bat
cd 1756-game
py -3 -m venv .venv
.venv\Scripts\activate.bat
python -m pip install --upgrade pip
python -m pip install -r requirements.txt
```

**PowerShell**

```powershell
cd 1756-game
py -3 -m venv .venv
.venv\Scripts\Activate.ps1
python -m pip install --upgrade pip
python -m pip install -r requirements.txt
```

If PowerShell blocks the activate script, allow scripts for your user once, then run the activate command again:

```powershell
Set-ExecutionPolicy -Scope CurrentUser RemoteSigned
```

**Git Bash**

```bash
cd 1756-game
py -3 -m venv .venv
source .venv/Scripts/activate
python -m pip install --upgrade pip
python -m pip install -r requirements.txt
```

Later sessions: open a terminal in the project directory and run the activate command for that shell (`.venv\Scripts\activate.bat`, `.venv\Scripts\Activate.ps1`, or `source .venv/Scripts/activate`).

## Run

Stay in the project directory. Image paths such as `images/nasasatelliteview.jpg` are relative to that folder. With the virtual environment activated:

```bash
python index.py
```

- **Play** opens the scenario picker. Click a card to start.
- **Codex** shows nations, flags, and infantry and cavalry sprites.
- **Escape** pauses. From the pause menu you can resume, return to the main menu, or exit.
- **F11** toggles fullscreen.
- Click cities, units, and the on-screen panels to inspect and give orders. The speed controls change how fast days pass.

## Project structure

```
1756-game/
├── index.py            Main loop, entities, rendering, events, and scenarios
├── config.py           Display, economy, movement, and balance constants
├── game_state.py       Shared lists: cities, units, wars, treaties, clock
├── cities_data.py      Starting settlements and later founded cities
├── historical_data.py  Demographics, religion, and population tables
├── rulers.py           Ruler timelines and portraits
├── warfare.py          War-score weights and diplomacy notes
├── collision.py        Land and water checks, A* pathfinding
├── sprites.py          Spritesheet loading for units, ships, and merchants
├── utilities.py        Regiment, settlement, and governor name generators
├── requirements.txt    Python packages for the virtual environment
├── .gitignore          Python bytecode, virtualenvs, and build output
└── images/             Map, collision mask, flags, sprites, and portraits
```

Edit `config.py` to change starting resources, unit costs, movement, disaster chances, treaty length, and war thresholds.

## Map and collision

- `images/nasasatelliteview.jpg` is the visual map. It has no effect on movement.
- `images/collisonmap.png` is the movement mask. The filename is spelled `collisonmap`. Red pixels are land. Blue pixels are water. Units, merchants, and settlers walk on land. Ships sail on water.
- `images/climates.png` sets the biome at each point.
- `images/regionmap.png` defines territories. Colored land inside black borders is a region. White is ocean.

`collision.py` exposes `is_water` and `is_land`, A* routes with `find_water_path` and `find_land_path`, and `path_needs_boat` when a land route has to cross water.
