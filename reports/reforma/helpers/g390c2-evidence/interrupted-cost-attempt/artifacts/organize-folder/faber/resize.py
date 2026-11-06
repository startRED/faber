from PIL import Image
import sys

img = Image.open(sys.argv[1])
img.thumbnail((800, 800))
img.save("small_" + sys.argv[1])
