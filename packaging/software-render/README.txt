UltraRushFTP - software rendering pack / pack de rendu logiciel
================================================================

ENGLISH

Only for computers where UltraRushFTP says "no usable graphics adapter": Windows without
Direct3D 12, such as Windows Server 2016 in a virtual machine without 3D acceleration.
Other computers do not need it.

Install: copy opengl32.dll into the folder of UltraRushFTP.exe (the installation folder,
by default C:\Program Files\UltraRushFTP, or the folder of the portable version), then
start UltraRushFTP. To remove it, delete that opengl32.dll.

The window is then drawn by the processor (Mesa llvmpipe, OpenGL): slower than a graphics
card, enough for UltraRushFTP. A graphics card, or the software renderer of Windows
(WARP), stays the first choice when one is available.

opengl32.dll is Mesa (https://mesa3d.org), built by https://github.com/mmozeiko/build-mesa;
licences in LICENSES.txt.


FRANÇAIS

Seulement pour les ordinateurs où UltraRushFTP affiche « no usable graphics adapter » :
Windows sans Direct3D 12, comme Windows Server 2016 dans une machine virtuelle sans
accélération 3D. Les autres ordinateurs n'en ont pas besoin.

Installation : copiez opengl32.dll dans le dossier de UltraRushFTP.exe (le dossier
d'installation, par défaut C:\Program Files\UltraRushFTP, ou le dossier de la version
portable), puis lancez UltraRushFTP. Pour l'enlever, supprimez ce opengl32.dll.

La fenêtre est alors dessinée par le processeur (Mesa llvmpipe, OpenGL) : plus lent qu'une
carte graphique, suffisant pour UltraRushFTP. Une carte graphique, ou le moteur logiciel de
Windows (WARP), reste le premier choix quand il y en a un.

opengl32.dll est Mesa (https://mesa3d.org), compilé par https://github.com/mmozeiko/build-mesa ;
licences dans LICENSES.txt.
