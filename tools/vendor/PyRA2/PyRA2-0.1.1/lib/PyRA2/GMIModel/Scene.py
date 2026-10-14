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

"""GMI Scene class"""

from Materials import MaterialList
from Objects import ObjectList

class Scene:
    """Scene"""
    _source = _path = ''
    _firstframe = _lastframe = _speed =_ticks = 0
    _static_bg = _static_ambient = (0.0, 0.0, 0.0)

    _materials = None
    _geometry = None

    def __init__(self):
        self._materials = MaterialList()
        self._objects = ObjectList()

    def getBasePath(self):
        """Directory where this model was imported from"""
        return self._path

    def getSourceFileName(self):
        """The sourcefile of the original ASE export."""
        return self._source

    def getFirstFrame(self):
        """Animation start frame"""
        return self._firstframe

    def getLastFrame(self):
        """Animation end frame"""
        return self._lastframe

    def getFrameSpeed(self):
        """Animation speed"""
        return self._speed

    def getTicksPerFrame(self):
        """Ticks per animation frame"""
        return self._ticks

    def getStaticBackgroundColor(self):
        """The static background color"""
        return self._static_bg

    def getStaticAmbientColor(self):
        """The static ambient color"""
        return self._static_ambient

    def getMaterialCount(self):
        return len(self._materials)

    def getMaterial(self, index):
        return self._materials[index]

    def getMaterials(self):
        return self._materials

    def getObjectCount(self):
        return len(self._objects)

    def getObject(self, index):
        return self._objects[index]

    def getObjects(self):
        return self._objects
