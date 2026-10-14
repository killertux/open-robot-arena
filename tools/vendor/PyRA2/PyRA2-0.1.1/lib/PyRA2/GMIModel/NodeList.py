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

"""Simple read-only list of objects"""

class NodeList:
    _items = ()

    def __len__(self): return len(self._items)
    def __repr__(self): return repr(self._items)
    def __getitem__(self, i): return self._items[i]

    def getItemByName(self, name):
        for item in self._items:
            if item.getName() == name:
                return item

    def getItemsByType(self, type):
        found = []
        for item in self._items:
            if item.__class__.__name__ == type:
                found.append(item)
        return tuple(found)
