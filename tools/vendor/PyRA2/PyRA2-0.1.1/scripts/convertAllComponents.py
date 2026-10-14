# PyRA2: Python support for Robot Arena 2 file formats.
# Copyright (C) 2003  Martijn Pieters <pyra2@zopatista.com>
#
# This library is free software; you can redistribute it and/or
# modify it under the terms of the GNU Lesser General Public
# License as published by the Free Software Foundation; either
# version 2.1 of the License, or (at your option) any later version.
#
# This library is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
# Lesser General Public License for more details.
#
# You should have received a copy of the GNU Lesser General Public
# License along with this library; if not, write to the Free Software
# Foundation, Inc., 59 Temple Place, Suite 330, Boston, MA  02111-1307  USA

from PyRA2.GMIModel.Import import loadScene
from PyRA2.GMIModel.VRMLExport import exportScene
import os.path

def exportGMI2VRML(input, output, show_hidden=0):
    scene = loadScene(input)
    exportScene(scene, output, heed_rbcoll=(not show_hidden))

def exportComponents(arg, dirname, names):
    for name in names:
        base, ext = os.path.splitext(name)
        if ext == '.gmf':
            if base.endswith('_txt'): continue
            output = open('%s.wrl' % base, 'w')
            input = open(os.path.join(dirname, name), 'rb')
            exportGMI2VRML(input, output)

base = r'C:\Program Files\Infogrames\Robot Arena 2\Components'
os.path.walk(base, exportComponents, None)
