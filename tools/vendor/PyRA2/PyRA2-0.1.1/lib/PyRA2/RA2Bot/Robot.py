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

"""Top-level Robot class"""

from Components import ComponentCollection
from Controllers import ControllerCollection
from Mesh import Mesh

# Weight classes
LIGHT, MEDIUM, HEAVY = range(3)

class Robot:
    _ra2version = (0, 0)
    _name = ''
    _weight_class = LIGHT
    _apply_default_texture = 1
    _heading = 0.0
    _snapshot = _components = _bashed_geometry = _controllers = None

    def __init__(self):
        self._components = ComponentCollection()
        self._controllers = ControllerCollection()

    def _createBashedMesh(self):
        self._bashed_geometry = Mesh()
        return self._bashed_geometry

    def getRA2Version(self):
        """Major, minor version of RA2 that did export"""
        return self._ra2version

    def getName(self):
        return self._name

    def getWeightClass(self):
        return self._weight_class

    def getWeight(self):
        weight = 0
        for comp in self._components:
            weight += comp.getWeight()
        return weight

    def applyDefaultTexture(self):
        """Should a default texture be applied when choosing an armour type?"""
        return self._apply_default_texture

    def getSnapshot(self):
        return self._snapshot

    def getHeading(self):
        """Forward heading in radians"""
        return self._heading

    def getComponents(self):
        return self._components

    def getComponent(self, index):
        return self._components[index]

    def getComponentCount(self):
        return len(self._components)

    def getChassis(self):
        return self.getComponent(0)

    def getBashedGeometry(self):
        return self._bashed_geometry

    def getControllers(self):
        return self._controllers

    def getController(self, x, y):
        return self._controllers[x, y]

    def getBindings(self):
        return self._bindings

    def getBinding(self, index):
        return self._bindings[index]

    def getBindingCount(self):
        return len(self._bindings)
