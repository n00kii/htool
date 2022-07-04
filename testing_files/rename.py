import os, binascii, sys

def rand_hex(digits):
    digits = digits // 2
    return binascii.b2a_hex(os.urandom(digits)).decode('utf-8')

root = sys.argv[1] if len(sys.argv) > 1 else '.'

# randomize all names
for path in os.listdir(root):
    path = os.path.join(root, path)
    _, ext = os.path.splitext(path)
    if ext == '.py': continue
    
    temp_filename = None

    while True:
        rand_name = rand_hex(6) + ext
        if not os.path.exists(rand_name): break

    new_path = os.path.join(root, rand_name)
    os.rename(path, new_path)

# enumerate names
i = 0
for path in os.listdir(root):
    path = os.path.join(root, path)
    _, ext = os.path.splitext(path)
    if ext == '.py': continue
    
    new_path = os.path.join(root, str(i) + ext)
    os.rename(path, new_path)
    i = i + 1