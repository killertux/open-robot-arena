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

"""Remote control for RA2 bots"""

from Collections import SimpleCollection

_controllerTypes = {}
class ControllerCollection(SimpleCollection):
    def _createController(self, controllerType):
        self._items += (_controllerTypes[controllerType](),)
        return self._items[-1]

    def getControllerByPos(self, pos):
        for controller in self._items:
            if controller.getPosition() == pos:
                return controller


class BaseController:
    _name = ''
    _pos = (0, 0)
    _type = 0
    _bindings = ()

    def getName(self):
        return self._name

    def getType(self):
        return self._type

    def getPosition(self):
        return self._pos

    def getBinding(self, index):
        return self._bindings[index]

    def getBindingCount(self):
        return self._bindings
    
    
class AnalogController(BaseController):
    _type = 'Analog'
    
    def __init__(self):
        self._bindings = (Binding(), Binding())

_controllerTypes[1] = AnalogController


class SwitchController(BaseController):
    _type = 'Switch'

    def __init__(self):
        self._bindings = (Binding(),)

_controllerTypes[2] = SwitchController


class ButtonController(BaseController):
    _type = 'Button'

    def __init__(self):
        self._bindings = (Binding(),)

_controllerTypes[4] = ButtonController


class Binding:
    _key = (0, 0, 0, 0)
    _component = -1
    _action = ''

    def getKey(self):
        return self._key

    def getComponent(self):
        return self._component

    def getAction(self):
        return self._action
