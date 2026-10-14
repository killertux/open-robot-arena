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

"""Robot file (.bot) model import

The loadRobot utility method will import the complete robot from the
filedescriptor passed in.

"""
import re

class ImportException(Exception):
    """Raised when finding inconsitencies in the imported file"""

def loadRobot(fd):
    """Load a RA2 robot from filedescriptor"""

    importer = RobotImporter(fd)
    return importer.loadRobot()

#
# Generic ascii file reader
#
class AsciiImporter:
    _fd = None
    _last = None

    def __init__(self, fd):
        self._fd = fd

    def _readLine(self):
        self._last = self._fd.readline().strip()
        return self._last

    def _err(self, msg):
        try:
            filename = self._fd.name
        except AttributeError:
            filename = repr(self._fd)
        return "%s (file %r, pos %i, last read %r)" % (
            msg, filename, self._fd.tell(), self._last)

    def readData(self, pattern):
        data = []
        fields = self._parsePattern(pattern)
        line = self._readLine()
        for field in fields:
            try:
                val, line = field(line)
            except:
                raise ImportException, self._err(
                    'Field parse error (%r)' % pattern)
            if val is not None:
                data.append(val)
        if line.strip():
            raise ImportError, self._err('Not all data on line used.')
        if len(data) == 1:
            return data[0]
        return tuple(data)

    _fields = {}
    _parse = re.compile("(?:[a-z]|'[^']+')", re.IGNORECASE).findall
    def _parsePattern(self, pattern):
        fields = []
        for field in self._parse(pattern):
            if field[0] == "'":
                fields.append(self._createLiteralField(field))
            else:
                fields.append(getattr(self, self._fields[field]))
        return fields

    def _createLiteralField(self, field):
        class literalField:
            def __init__(self, literal):
                self._lit = literal.strip()[1:-1]
            def __call__(self, line):
                if line.startswith(self._lit):
                    return None, line[len(self._lit):].strip()
                else:
                    raise ImportException, self._err('Expected %r')
        return literalField(field)

    def _convertField(self, line, converter):
        val, line = (line + ' ').split(' ', 1)
        return converter(val), line.strip()

    def _intField(self, line):
        return self._convertField(line, int)
    _fields['i'] = '_intField'

    def _floatField(self, line):
        return self._convertField(line, float)
    _fields['f'] = '_floatField'

    _bool = re.compile('(true|false) ?(.*)').match
    def _boolField(self, line):
        bool, line = self._bool(line).groups()
        return bool == 'true', line
    _fields['b'] = '_boolField'

    def _stringField(self, line):
        return line, ''
    _fields['s'] = '_stringField'

    def readInt(self):
        try:
            return int(self._readLine())
        except ValueError:
            raise ImportException, self._err('Expected integer')

    def readFloat(self):
        try:
            return float(self._readLine())
        except ValueError:
            raise ImportException, self._err('Expected float')

    def readString(self):
        return self._readLine()

    def readBoolean(self):
        val = self._readLine()
        if val not in ('true', 'false'):
            raise ImportException, self._err('Expected boolean')
        return val == 'true'

    def readImage(self, dimension):
        import Image
        from cStringIO import StringIO
        length = 18 + 4 * dimension * dimension
        imagedata = ''
        while len(imagedata) < length:
            imagedata += self._fd.read(length - len(imagedata))
        im = Image.open(StringIO(imagedata))
        opaque = Image.new('L', im.size, 255)
        im.putalpha(opaque)
        return im

_componentImporters = {}
class RobotImporter(AsciiImporter):
    _version = (0, 0)

    def loadRobot(self):
        from Robot import Robot
        bot = Robot()

        # Header
        major, minor = self.readString().split('.')
        major, minor = int(major), int(minor)
        if major != 1 or minor not in (7, 12):
            raise ImportException, self._err('Unsupported version')
        bot._ra2version = (major, minor)
        self._version = (major, minor)

        bot._name = self.readData("'Name:' s")
        bot._weight_class = self.readData("'Class:' i")
        if self._version > (1, 7):
            bot._apply_default_texture = self.readInt()
        if self.readBoolean():
            bot._snapshot = self.readImage(128)

        self.readComponents(bot._components)

        if self._version > (1, 7):
            # Bashed model
            if self.readBoolean():
                meshImporter = MeshImporter(self._fd)
                meshImporter.loadMesh(bot._createBashedMesh())

            # Heading
            bot._heading = self.readFloat()

        self.readControllers(bot._controllers)
        self.readBindings(bot._controllers)

        return bot

    def readComponents(self, components):
        for i in range(self.readInt()):
            parent = self.readInt()
            parentMount = self.readInt()
            attachmentPoint = self.readInt()
            translation = self.readData('f f f')
            rotation = self.readData('f f f f')
            angle, height = self.readData('f f')
            class_ = self.readString()
            datafile = self.readString()

            component = components._createComponent(class_, parent,
                parentMount, attachmentPoint, translation, rotation, angle,
                height, datafile)

            importer = _componentImporters.get(class_, None)
            if importer: # More data to be read
                importer = importer(self._fd, self._version)
                importer.loadComponentData(component)

    def readControllers(self, controllers):
        for i in range(self.readInt()):
            name = self.readString()
            x, y, type = self.readData('i i i')
            controller = controllers._createController(type)
            controller._name = name
            controller._pos = (x, y)

            for j in range(self.readInt()):
                if self._version > (1, 7):
                    key = self.readData('i i i i')
                else:
                    key = (0,) + self.readData('i i i')
                controller.getBinding(j)._key = key

    def readBindings(self, controllers):
        for i in range(self.readInt()):
            component = self.readInt()
            action = self.readString()
            x, y, index = self.readData('i i i')
            controller = controllers.getControllerByPos((x, y))
            binding = controller.getBinding(index)
            binding._component = component
            binding._action = action


class BaseComponentImporter(AsciiImporter):
    _version = (0, 0)

    def __init__(self, fd, version):
        AsciiImporter.__init__(self, fd)
        self._version = version

    def loadComponentData(self, component):
        raise NotImplementedError


class ChassisImporter(BaseComponentImporter):
    def loadComponentData(self, chassis):
        facecount = self.readTopBottomGeom(chassis)
        
        if self.readBoolean():
            meshImporter = MeshImporter(self._fd)
            meshImporter.loadMesh(chassis._createMesh(), facecount)
            self.readTextureData(chassis)
            self.readTopCorners(chassis)

        self.readArmourData(chassis)

        if self._version > (1, 7):
            self.readBucklingPoints(chassis)

    def readTopBottomGeom(self, chassis):
        # Bottom and top geometry
        points = []
        for i in range(self.readInt()):
            points.append(self.readData('i i'))
        chassis._platePoints = tuple(points)

        points = []
        for i in range(self.readInt()):
            points.append(self.readData('i i'))
        chassis._topPoints = tuple(points)

        chassis._height, facecount = self.readData('f i')

        return facecount

    def readTextureData(self, chassis):
        if self.readBoolean():
            # Texture map
            chassis._textureMap = self.readImage(256)

        if self._version > (1, 7):
            unknown = self.readBoolean()
            assert not unknown, self._err('Unknown section flag true?')

        # Face groups
        groupcount = self.readInt()
        groupTexCoords = []
        for i in range(groupcount):
            coords = []
            for j in range(self.readInt()):
                coords.append(self.readData('f f'))
            groupTexCoords.append(tuple(coords))
        chassis._groupTexCoords = tuple(groupTexCoords)

        boxcount = self.readInt()
        assert boxcount == groupcount
        boundingBoxes = []
        for i in range(groupcount):
            index, x1, y1, x2, y2 = self.readData('i i i i i')
            assert index == len(boundingBoxes)
            boundingBoxes.append(((x1, y1), (x2, y2)))
        chassis._boundingBoxes = tuple(boundingBoxes)

        faceGroups = {}
        for i in range(self.readInt()):
            index, a, b, c = self.readData('i i i i')
            group = faceGroups.setdefault(index, [])
            group.append((a, b, c))
        for i in range(groupcount):
            chassis._faceGroups += (tuple(faceGroups[i]),)

    def readTopCorners(self, chassis):
        corners = []
        for i in range(self.readInt()):
            index, a, b, c = self.readData("i '3' i i i")
            assert index == i
            corners.append((a, b, c))
        chassis._topCorners = tuple(corners)

    def readArmourData(self, chassis):
        type, weight, hardness = self.readData("i i '0' i '0'")
        chassis._armour = type
        chassis._armourweight = weight
        chassis._armourhardness = hardness

        if chassis.getMesh():
            chassis._unknown = self.readData('f f f f i')

    def readBucklingPoints(self, chassis):
        bucklepoints = []
        for i in range(self.readInt()):
            point = [self.readData('f f f')]
            for j in range(self.readInt()):
                point.append(self.readInt())
            bucklepoints.append(tuple(point))
        chassis._bucklePoints = tuple(bucklepoints)

_componentImporters['Chassis'] = ChassisImporter


class MeshImporter(AsciiImporter):
    def loadMesh(self, mesh, sizehint=None):
        tot_points, tot_indexes, tot_faces = self.readData("'274' i i i")
        if sizehint is not None:
            assert tot_faces == sizehint, self._err('Inconsistent face count?')

        meshcount = self.readInt()
        assert meshcount == 1, self._err('More than one mesh?')

        indexcount, facecount, pointcount = self.readData("i i i '0'")
        assert indexcount == tot_indexes
        assert facecount == tot_faces
        assert pointcount == tot_points
        assert facecount * 3 == indexcount

        raw = self.readString()
        assert raw == 'RAW'

        normals = []
        for i in range(pointcount):
            normals.append(self.readData('f f f'))
        mesh._normals = tuple(normals)

        texCoords = []
        for i in range(pointcount):
            texCoords.append(self.readData('f f'))
        mesh._texCoords = tuple(texCoords)

        points = []
        for i in range(pointcount):
            points.append(self.readData('f f f'))
        mesh._points = tuple(points)

        faces = []
        for i in range(facecount):
            faces.append((self.readInt(), self.readInt(), self.readInt()))
        mesh._faces = tuple(faces)


class BurstMotorImporter(BaseComponentImporter):
    def loadComponentData(self, motor):
        motor._startAngle, motor._stopAngle = self.readData('f f')

_componentImporters['BurstMotor'] = BurstMotorImporter


class SmartZoneImporter(BaseComponentImporter):
    def loadComponentData(self, zone):
        zone._name = self.readString()

_componentImporters['SmartZone'] = SmartZoneImporter
