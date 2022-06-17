import os, binascii

abspath = os.path.abspath('.')
count = 0

def rand_hex(digits):
    digits = digits // 2
    return binascii.b2a_hex(os.urandom(digits)).decode('utf-8')

for entry in os.scandir('.'):
    _, ext = os.path.splitext(entry.path)
    if ext == '.py': continue
    
    temp_filename = None

    while True:
        rand_name = rand_hex(6) + ext
        if not os.path.exists(rand_name): break

    os.rename(entry.path, rand_name)

for (i, entry) in enumerate(os.scandir('.')):
    _, ext = os.path.splitext(entry.path)
    if ext == '.py': continue
    
    os.rename(entry.path, str(i) + ext)
