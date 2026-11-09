import struct,json
f=open('models/avatars/vroid-sample-f.vrm','rb')
h=f.read(12); print('header:',h[:4],struct.unpack('<I',h[4:8])[0])
clen=struct.unpack('<I',f.read(4))[0]; ctype=f.read(4).decode()
print('chunk0:',ctype,'len=',clen)
j=json.loads(f.read(clen))
print('scenes:',len(j.get('scenes',[])))
for i in range(len(j.get('scenes',[]))):
    print('  scene %d: nodes=%s'%(i,j['scenes'][i].get('nodes')))
def dump(n,d):
    indent=' '*d; name=n.get('name','id='+str(n.get('id'))); mi=n.get('mesh'); ch=n.get('children',[])
    print('%s%s mesh=%s children=%d'%(indent,name,mi,len(ch)))
    for c in ch: dump(c,d+1)
for n in j.get('nodes',[]): dump(n,0)
print('exts:',list(j.get('extensions',{}).keys()))
for i,m in enumerate(j.get('meshes',[])):
    print('  mesh %d: name=%s'%i,m.get('name','unnamed'))
