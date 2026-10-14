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
import os.path

def extractComponentWeight(rbodies):
    weight = 0
    for rbody in rbodies:
        weight += rbody.getMass()
        weight += extractComponentWeight(rbody.getChildren())
    return weight

def extractComponentWeights(arg, dirname, names):
    for name in names:
        base, ext = os.path.splitext(name)
        if ext == '.gmf':
            if base.endswith('_txt'): continue
            scene = loadScene(open(os.path.join(dirname, name), 'rb'))
            rbcolls = scene.getObjects().getItemsByType('RigidBodyCollection')
            if rbcolls:
                weight = extractComponentWeight(rbcolls[0].getRigidBodies())
                print '    %r: %g,' % (base, weight)

base = r'C:\Program Files\Infogrames\Robot Arena 2\Components'
print '''"""Component weights extracted from the Component directory.

Extraction done automatically by extractWeights.py. Keys are the VRML file
names; use ComponentReferences.COMPONENTS to convert from component file names
in .bot files.

"""
COMPONENT_WEIGHTS = {'''
os.path.walk(base, extractComponentWeights, None)
print '}'
