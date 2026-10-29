import os,json,hashlib,stat,sqlite3,sys
from pathlib import Path
r=Path(json.loads(Path('/tmp/hub-launch-qa-path.json').read_text())['root'])
c=sqlite3.connect(f'file:{r}/metadata/metadata.sqlite?mode=ro',uri=True);instance=Path(c.execute('select data_root from play_instance').fetchone()[0]);c.close()
scopes={'h_game':r/'H-Game.app','i_game':r/'Game.app','instances':r/'metadata.workspace/instances','sentinel':r/'sentinel','h_source':Path('/private/tmp/cataclysm-hub-real-game-validation/0.H-RELEASE.app')}
result={'scope':'declared directories only; symlinks recorded, not traversed','roots':{}}
for name,root in scopes.items():
 entries={}
 for base,dirs,files in os.walk(root,followlinks=False):
  for leaf in dirs+files:
   p=Path(base)/leaf;s=p.lstat();item={'mode':stat.S_IMODE(s.st_mode),'kind':'link' if p.is_symlink() else 'directory' if p.is_dir() else 'file'}
   if p.is_symlink():item['target']=os.readlink(p)
   elif p.is_file():item['size']=s.st_size;item['sha256']=hashlib.file_digest(p.open('rb'),'sha256').hexdigest()
   entries[str(p.relative_to(root))]=item
 result['roots'][name]={'path':str(root),'identity':[root.stat().st_dev,root.stat().st_ino],'entries':entries}
(r/'evidence'/f'{sys.argv[1]}.json').write_text(json.dumps(result,indent=2));print({k:len(v['entries']) for k,v in result['roots'].items()})
