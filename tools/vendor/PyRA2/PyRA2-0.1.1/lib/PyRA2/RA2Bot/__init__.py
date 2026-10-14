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

"""Robot Arena 2 Robot (.bot) file support

Robot designs are stored in .bot files. The files basically hold newlines
seperated data with the occasional uncrompressed TGA-encoded bitmap.

This module provides for a set of classes to import and hold the robot model 
contained in a .bot file.

"""

from Import import loadRobot
from Robot import LIGHT, MEDIUM, HEAVY

# Allow Chassis to register itself as a component class
import Chassis
