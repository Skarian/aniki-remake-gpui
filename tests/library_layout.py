"""Read-only importer regression: separate program/data/library paths, no bundled DLLs."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--importer', type=Path, required=True)
p.add_argument('--playnite', type=Path, default=Path('D:/Playnite'))
p.add_argument('--launcher', type=Path)
a = p.parse_args()
out = Path(__file__).resolve().parents[1] / 'artifacts/library-layout-tests'
out.mkdir(parents=True, exist_ok=True)
scratch = Path(tempfile.mkdtemp(prefix='run-', dir=out))
data, library, tools, clean = [scratch / n for n in ('User Data', 'Custom Library', 'fixture-tools', 'clean-importer')]
for directory in (data, library, tools, clean):
    directory.mkdir()
# Test-only database generator. This user-provided DLL is not in the candidate package.
shutil.copy2(a.playnite / 'LiteDB.dll', tools / 'LiteDB.dll')
source = tools / 'Fixture.cs'
source.write_text('''using LiteDB;
class Fixture { static void Main(string[] args) {
 using(var db=new LiteDatabase(args[0])) {
  var game=new BsonDocument(); game["Name"]="Split folder fixture";
  game["IsInstalled"]=true; game["CoverImage"]="cover.png";
  db.GetCollection("games").Insert(game);
 }
}}''')
csc = Path('C:/Windows/Microsoft.NET/Framework64/v4.0.30319/csc.exe')
subprocess.run([str(csc), '/nologo', '/out:' + str(tools/'Fixture.exe'),
                '/reference:' + str(tools/'LiteDB.dll'), str(source)], check=True)
database = library / 'games.db'
subprocess.run([str(tools/'Fixture.exe'), str(database)], check=True)
before = hashlib.sha256(database.read_bytes()).hexdigest()
shutil.copy2(a.importer, clean / 'PlayniteFullscreen.Import.exe')
assert list(clean.glob('*.dll')) == []
snapshot = scratch / 'snapshot.json'
subprocess.run([str(clean/'PlayniteFullscreen.Import.exe'), str(a.playnite.resolve()), str(snapshot),
                str(library), str(data)], check=True, capture_output=True, text=True)
raw = json.loads(snapshot.read_text())
assert Path(raw['root']) == a.playnite.resolve(), raw
assert Path(raw['data_root']) == data, raw
assert Path(raw['library']) == library, raw
assert raw['collections']['games'][0]['Name'] == 'Split folder fixture', raw
assert hashlib.sha256(database.read_bytes()).hexdigest() == before, 'Importer changed database'
assert list(clean.glob('*.dll')) == [], 'Importer copied a Playnite DLL'
if a.launcher:
    normalized = subprocess.run([str(a.launcher.resolve()), '--check-snapshot', str(snapshot)],
                                check=True, capture_output=True, text=True)
    games = json.loads(normalized.stdout)['games']
    assert games[0]['name'] == 'Split folder fixture', games
    assert Path(games[0]['cover']) == library/'files/cover.png', games
# Legacy two-argument invocation still describes a portable layout.
portable_library = a.playnite / 'library'
if portable_library.is_dir():
    legacy = scratch / 'portable.json'
    subprocess.run([str(clean/'PlayniteFullscreen.Import.exe'), str(a.playnite.resolve()), str(legacy)],
                    check=True, capture_output=True, text=True)
    portable = json.loads(legacy.read_text())
    assert portable['root'] == portable['data_root']
    assert Path(portable['library']) == portable_library.resolve()
print('PASS: split paths, custom library, read-only import, no adjacent Playnite DLLs, portable compatibility')
