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

"""Robot components"""

from Collections import SimpleCollection
from ComponentReferences import COMPONENTS
from ComponentWeights import COMPONENT_WEIGHTS

_componentClasses = {}
def registerComponentClass(class_):
    _componentClasses[class_.__name__] = class_

class ComponentCollection(SimpleCollection):
    def _createComponent(self, class_, *args, **kw):
        type_ = _componentClasses.get(class_, SimpleComponent)
        self._items += (type_(len(self._items), class_, *args, **kw),)
        return self._items[-1]

    def getComponentsByParent(self, parent):
        children = []
        for item in self._items:
            if item.getParent() == parent:
                children.append(item)
        return tuple(children)

class SimpleComponent:
    _class = _datafile = ''
    _id = _parent = _parentMountPoint = _attachmentPoint = 0
    _translation = (0.0, 0.0, 0.0)
    _rotation = _angle = _height = 0.0

    def __init__(self, id, class_, parent, parentMount, attachmentPoint,
                 translation, rotation, angle, height, datafile):
        self._id = id
        self._class = class_
        self._parent = parent
        self._parentMountPoint = parentMount
        self._attachmentPoint = attachmentPoint
        self._translation = translation
        self._rotation = rotation
        self._angle = angle
        self._height = height
        self._datafile = datafile

    def getId(self):
        return self._id

    def getClass(self):
        return self._class
    
    def getParent(self):
        return self._parent

    def getParentMountPoint(self):
        return self._parentMountPoint

    def getAttachmentPoint(self):
        return self._attachmentPoint

    def getTranslation(self):
        return self._translation

    def getRotation(self):
        return self._rotation

    def getAngle(self):
        return self._angle

    def getHeight(self):
        return self._height

    def getDataFile(self):
        return self._datafile

    def getWeight(self):
        return COMPONENT_WEIGHTS[COMPONENTS[self._datafile]]


class BurstMotor(SimpleComponent):
    _startAngle = _stopAngle = 0.0

    def getStartAngle(self):
        return self._startAngle

    def getStopAngle(self):
        return self._stopAngle

registerComponentClass(BurstMotor)


class SmartZone(SimpleComponent):
    _name = ''

registerComponentClass(SmartZone)
