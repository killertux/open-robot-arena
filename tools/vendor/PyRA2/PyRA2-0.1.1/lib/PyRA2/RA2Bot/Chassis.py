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

"""Chassis component"""

from Components import SimpleComponent, registerComponentClass
from Mesh import Mesh

class Chassis(SimpleComponent):
    _platePoints = _topPoints = _groupTexCoords = _boundingBoxes = ()
    _faceGroups = _topCorners = _bucklePoints = ()
    _height = _armourweight = _weight = _armourhardness = 0.0
    _armour = 0

    _mesh = None
    _textureMap = None

    def _createMesh(self):
        self._mesh = Mesh()
        return self._mesh

    def _calcWeight(self):
        if not self._mesh: return
        self._weight = self._mesh.getArea() * self._armourweight

    def getMesh(self):
        return self._mesh

    def getTextureMap(self):
        return self._textureMap

    def getWeight(self):
        if not self._weight:
            self._calcWeight()
        return self._weight

registerComponentClass(Chassis)
