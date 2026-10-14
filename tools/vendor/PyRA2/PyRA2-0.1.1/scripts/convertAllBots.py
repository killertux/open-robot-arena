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

from PyRA2.RA2Bot.Import import loadRobot
from PyRA2.RA2Bot.VRMLExport import exportRobot
import os.path

def exportRobot2VRML(input):
    robot = loadRobot(input)

    baseName = robot.getName().replace(' ', '_').replace('.', '_')
    output = open('%s.wrl' % baseName, 'w')

    texName = '%s.png' % baseName
    texPath = os.path.dirname(os.path.abspath(output.name))
    texFile = open(os.path.join(texPath, texName), 'wb')

    if robot.getSnapshot():
        snapFile = '%s_snap.png' % baseName
        robot.getSnapshot().save(snapFile, optimize=1)

    exportRobot(robot, output, tex_fd=texFile, tex_filename=texName,
                comp_path='..\\Components\\')

def exportBots(arg, dirname, names):
    for name in names:
        base, ext = os.path.splitext(name)
        if ext == '.bot':
            input = open(os.path.join(dirname, name), 'rb')
            exportRobot2VRML(input)

base = r'C:\Program Files\Infogrames\Robot Arena 2'
os.path.walk(base, exportBots, None)
