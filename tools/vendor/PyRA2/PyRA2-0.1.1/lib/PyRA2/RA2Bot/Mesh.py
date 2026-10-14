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

"""Geometric mesh"""

from math import sqrt

class Mesh:
    _points = _normals = _texCoords = _faces = ()

    def getPoints(self):
        return self._points

    def getNormals(self):
        return self._normals

    def getTextureCoords(self):
        return self._texCoords

    def getFaces(self):
        return self._faces

    def getArea(self):
        """Surface area of mesh"""
        total = 0
        for face in self._faces:
            total += self._face_area(*face)
        return total
            
    def _face_area(self, a, b, c):
        """Surface area of a face"""
        a, b, c = self._points[a], self._points[b], self._points[c]
        a, b, c = self._dist(a, b), self._dist(b, c), self._dist(c, a)
        s = 0.5 * (a + b + c)
        return sqrt(s * (s - a) * (s - b) * (s - c))

    def _dist(self, a, b):
        """Distance between two points"""
        x, y, z = b[0] - a[0], b[1] - a[1], b[2] - a[2]
        return sqrt(x * x  + y * y + z * z)
