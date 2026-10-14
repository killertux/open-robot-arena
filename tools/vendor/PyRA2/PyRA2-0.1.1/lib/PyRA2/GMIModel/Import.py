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

"""GMI (.gmf) model import

The loadScene utility method will import a complete scene from the
filedescriptor passed in.

"""

from struct import unpack
from os.path import dirname

class ImportException(Exception):
    """Raised when finding inconsitencies in the imported file"""

def loadScene(fd):
    """Load a GMI 3D scene from filedescriptor."""

    if hasattr(fd, 'name'):
        dir = dirname(fd.name)
    else:
        dir = ''
    importer = SceneImporter(fd, dir)
    return importer.loadScene()

#
# Generic binary file reader
#
class BinaryImporter:
    _fd = None
    _last = None
    
    def __init__(self, fd):
        self._fd = fd

    def _read(self, len):
        self._last = self._fd.read(len)
        return self._last

    def readInt(self):
        """Read an integer"""
        return unpack('i', self._read(4))[0]

    def readFloat(self):
        """Read a float"""
        return unpack('f', self._read(4))[0]

    def readString(self):
        """Read a string"""
        length = self.readInt()
        value = self._read(length)
        if length > 0:
            value = value[:-1] # Strip off NULL, serves no purpose in Python
        return value

    def readByte(self):
        """Read unsigned int byte"""
        return unpack('B', self._read(1))[0]
    readBoolean = readByte

    def readColor(self):
        """Read a 4 byte RGB color

        Return as 0.0-1.0 float RGB values. Note: values stored in bgr order.

        """
        rgb = list(unpack('BBBx', self._read(4)))
        return tuple([c/255.0 for c in rgb])

    def readFloatTriple(self):
        """Read a 3 float values"""
        return unpack('fff', self._read(12))
    readVector = readFloatTriple
    readPoint = readVector

    def readIndexSet(self):
        """Read a 3-int index set"""
        return unpack('iii', self._read(12))
    readTriplet = readIndexSet

    def readTM(self):
        """Read a 3x4 transformation matrix"""
        return (self.readFloatTriple(), self.readFloatTriple(),
                self.readFloatTriple(), self.readFloatTriple())

    def _err(self, msg):
        """Convenience method for generating error messages."""
        try:
            filename = self._fd.name
        except AttributeError:
            filename = repr(self._fd)
        return "%s (file %r, pos %i, last read %r)" % (
            msg, filename, self._fd.tell(), self._last)

#
# Import a complete scene
#
class SceneImporter(BinaryImporter):
    """GMI Scene importer"""
    _scene = None
    
    def __init__(self, fd, dirname):
        from Scene import Scene
        BinaryImporter.__init__(self, fd)
        self.readHeader()
        self._scene = Scene()
        self._scene._path = dirname

    def loadScene(self):
        """Import complete scene"""

        self.readScene()
        self.readMaterialList()
        self.readObjectList()

        if self._read(1) != '':
            raise ImportException, self._err("We didn't parse the whole file")

        return self._scene

    def readHeader(self):
        """Read the initial header.

        Raises ValueError if file is not recognized as a GMI file.

        """
        self._fd.seek(0)
        magic = self._read(3)
        if magic != 'GMI':
            self._fd.seek(0)
            raise ValueError('Not a GMI file!')
        
        err = 'Unexpected header data'
        assert self.readInt() == 3, self._err(err) # Version
        assert self.readInt() == 1, self._err(err) # Model type (Basic)
        assert self.readInt() == 0, self._err(err) # ??
        assert self.readInt() == 15, self._err(err) # ??

    def readScene(self):
        """Read all data from the scene node"""
        id, flags, length = self.readTriplet()
        start = self._fd.tell()
        assert id == 1, self._err('Expected Scene node')
        assert flags == 2, self._err('Scene node flags not 2')

        self._scene._source = self.readString()
        self._scene._firstframe = self.readInt()
        self._scene._lastframe = self.readInt()
        self._scene._speed = self.readInt()
        self._scene._ticks = self.readInt()
        self._scene._static_bg = self.readColor()
        self._scene._static_ambient = self.readColor()

        # Make sure we finished reading the section
        assert self._fd.tell() == start + length, self._err(
            'Unexpected Scene Node length mismatch')

    def readMaterialList(self):
        """Read the material list section"""
        importer = MaterialListImporter(self._fd, self._scene._materials)
        importer.readMaterialList()

    def readObjectList(self):
        """Read the object list section"""
        importer = ObjectListImporter(self._fd, self._scene._objects)
        importer.readObjectList()

#
# Import Material List node
#
class MaterialListImporter(BinaryImporter):
    _materialList = None

    def __init__(self, fd, materialList):
        BinaryImporter.__init__(self, fd)
        self._materialList = materialList

    def readMaterialList(self, sizehint=None):
        """Read the material list section"""
        id, flags, length = self.readTriplet()
        start = self._fd.tell()
        assert id == 7, self._err('Expected Material List node')
        assert flags == 2, self._err('Material List node flags not 2')

        count = self.readInt()
        if sizehint is not None:
            assert count == sizehint, self._err(
                "Size hint didn't match actual count")

        for i in range(count):
            importer = MaterialImporter(self._fd)
            importer.readMaterial(self._materialList, i)

        assert self._fd.tell() == start + length, self._err(
            'Unexpected Material List node length mismatch')

#
# Material Node
#
class MaterialImporter(BinaryImporter):
    def readMaterial(self, materialList, index):
        id, flags, length = self.readTriplet()
        start = self._fd.tell()
        assert id == 8, self._err('Expected Material node')
        assert flags == 2, self._err('Material node flags not 2')

        ref_no = self.readInt()
        assert ref_no == index, self._err(
            'Material reference number out of sync?')

        name = self.readString()
        klass = self.readString()

        material = materialList._createMaterial(klass)
        material._name = name

        material._ambient = self.readColor()
        material._diffuse = self.readColor()
        material._specular = self.readColor()

        material._shine = self.readFloat()
        material._shine_str = self.readFloat()
        material._trans = self.readFloat()
        material._wire = self.readFloat()
        material._shading = self.readInt()
        material._xp_falloff = self.readFloat()
        material._self_illum = self.readFloat()

        # It may be that the 4 bytes for two-sided are actually two 2 byte
        # fields; one for two-sided, one for falloff; I have yet to come across
        # an example though. It is certain that the first byte at the very
        # least pertains to two-sided-ness.
        material._two_sided = self.readInt()
        material._falloff = 0

        material._xp_type = self.readInt()

        texture_count = self.readInt()
        if texture_count:
            assert klass == 'Standard'
            importer = TextureListImporter(self._fd, material._textures)
            importer.readTextureList(texture_count)

        sub_count = self.readInt()
        if sub_count:
            assert klass == 'Multi/Sub-Object'
            importer = MaterialListImporter(self._fd, material._sub_materials)
            importer.readMaterialList(sub_count)

        assert self._fd.tell() == start + length, self._err(
            'Unexpected Material node length mismatch')
        

#
# Import Texture List node
#
class TextureListImporter(BinaryImporter):
    _textureList = None

    def __init__(self, fd, textureList):
        BinaryImporter.__init__(self, fd)
        self._textureList = textureList

    def readTextureList(self, sizehint=None):
        """Read the texture list section"""
        id, flags, length = self.readTriplet()
        start = self._fd.tell()
        assert id == 14, self._err('Expected Texture List node')
        assert flags == 2, self._err('Texture List node flags not 2')

        count = self.readInt()
        if sizehint is not None:
            assert count == sizehint, self._err(
                "Size hint didn't match actual count")

        for i in range(count):
            importer = TextureImporter(self._fd)
            importer.readTexture(self._textureList, i)

        assert self._fd.tell() == start + length, self._err(
            'Unexpected Texture List node length mismatch')

#
# Texture Node
#
class TextureImporter(BinaryImporter):
    def readTexture(self, textureList, index):
        id, flags, length = self.readTriplet()
        start = self._fd.tell()
        assert id == 15, self._err('Expected Texture node')
        assert flags == 4, self._err('Texture node flags not 4')

        name = self.readString()
        klass = self.readString()

        texture = textureList._createTexture(klass)
        texture._name = name

        # Most of the following only applies to bitmap textures, but the GMI
        # file contains these fields for all texture types.
        texture._bitmap = self.readString()
        texture._amount = self.readFloat()
        texture._type = self.readInt()
        texture._map_type = self.readInt()
        texture._u_offset = self.readFloat()
        texture._v_offset = self.readFloat()
        texture._u_tiling = self.readFloat()
        texture._v_tiling = self.readFloat()
        texture._angle = self.readFloat()
        texture._blur = self.readFloat()
        texture._blur_offset = self.readFloat()
        texture._noise = self.readFloat()
        texture._noise_size = self.readFloat()
        texture._noise_level = self.readInt()
        texture._noise_phase = self.readFloat()

        # The following two fields are speculative; all RA2 files list these as
        # 0. The ASCII export source code shows a inversion export, but no
        # other field.
        texture._invert = self.readInt()
        texture._unknown = self.readInt()

        texture._filter = self.readInt()
        texture._channel = self.readInt()

        sub_count = self.readInt()
        if sub_count:
            importer = TextureListImporter(self._fd, texture._sub_textures)
            importer.readTextureList(sub_count)

        assert self._fd.tell() == start + length, self._err(
            'Unexpected Texture node length mismatch')

#
# Transformable node importer base
#
class TransformableNodeImporter(BinaryImporter):
    def readTMNode(self, _expectedName=None):
        """Read a Transformation matrix node and return the TM."""
        id, flags, length = self.readTriplet()
        start = self._fd.tell()
        assert id == 17, self._err('Expected Transformation Matrix node')
        assert flags == 2, self._err('Transformation Matrix node flags not 2')

        tm_name = self.readString()
        if _expectedName:
            assert _expectedName == tm_name

        matrix = self.readTM()

        assert self._fd.tell() == start + length, self._err(
            'Unexpected Texture node length mismatch')
        return matrix


#
# Import Object List node
#
objectImporters = {}
class ObjectListImporter(BinaryImporter):
    _objectList = None

    def __init__(self, fd, objectList):
        BinaryImporter.__init__(self, fd)
        self._objectList = objectList

    def readObjectList(self):
        """Read the object list section"""
        id, flags, length = self.readTriplet()
        start = self._fd.tell()
        assert id == 18, self._err('Expected Object List node')
        assert flags == 2, self._err('Object List node flags not 2')

        count = self.readInt()

        for i in range(count):
            object_id, object_flags, section_length = self.readTriplet()
            importer = objectImporters.get(object_id, None)
            if not importer:
                raise ImportException, self._err('Unknown object node type')
            importer(self._fd).readObjectNode(self._objectList, object_flags,
                                              section_length)

        # Current GMI files have the object list section lenght incorrect
        # because various object nodes report 0 or incorrect lenght.
        # assert self._fd.tell() == start + length, self._err(
        #     'Unexpected Object List node length mismatch')


#
# Geometry Node import
#
class GeometricObjectNodeImporter(TransformableNodeImporter):
    def readObjectNode(self, objectList, flags, length):
        assert flags == 4, self._err('Geometry Node flags not 4')
        start = self._fd.tell()
        
        geom = objectList._createObject('GeometricObject')
        geom._name = self.readString()
        geom._parent = self.readString()
        geom._shade_verts = self.readBoolean()
        geom._tm = self.readTMNode(geom.getName())
        self.readMesh(geom.getMesh())

        # Current GMI files have the geom node section lenght incorrect
        # because it's mesh node reports 3 to 7 bytes too short.
        # assert self._fd.tell() == start + length, self._err(
        #    'Unexpected Geometric Object node length mismatch')

    def readMesh(self, mesh):
        id, flags, length = self.readTriplet()
        start = self._fd.tell()
        assert id == 16, self._err('Expected Mesh node')
        assert flags == 4, self._err('Object List node flags not 2')

        mesh._time = self.readInt()
        pointcount = self.readInt()
        facecount = self.readInt()
        texturecount = self.readInt()
        colorcount = self.readInt()
        mesh._materialRef = self.readInt()

        points = []
        for i in range(pointcount):
            points.append(self.readPoint())
        mesh._points = tuple(points)

        faces = []
        for i in range(facecount):
            faces.append(self.readIndexSet() + (self.readInt(),))
        mesh._faces = tuple(faces)

        textureCoords = []
        for i in range(texturecount):
            textureCoords.append(self.readFloatTriple())
        mesh._textureCoords = tuple(textureCoords)

        if texturecount:
            textureFaces = []
            for i in range(facecount):
                textureFaces.append(self.readIndexSet())
            mesh._textureFaces = tuple(textureFaces)

        additional_channels = self.readInt()
        if additional_channels:
            self.readTextureChannels(mesh._textureChannels)

        colors = []
        for i in range(colorcount):
            colors.append(self.readFloatTriple())
        mesh._colors = tuple(colors)

        if colorcount:
            colorFaces = []
            for i in range(facecount):
                colorFaces.append(self.readIndexSet())
            mesh._colorFaces = tuple(colorFaces)

        normals = []
        for i in range(facecount):
            for j in range(4):
                normals.append(self.readVector())
        mesh._normals = tuple(normals)

        mesh._backface_cull = self.readInt()

    def readTextureChannels(self, textureChannels):
        for i in range(self.readInt()):
            assert i + 2 == self.readInt(), self._err(
                'Texture Channel index off?')
            channel = textureChannels._createChannel()
            texturecount = self.readInt()
            assert 0 == self.readInt(), self._err(
                'Channel Int 2 not 0') # XXX
            facecount = self.readInt()
            assert 0 == self.readInt(), self._err(
                'Channel Int 4 not 0') # XXX
            assert 0 == self.readInt(), self._err(
                'Channel Int 5 not 0') # XXX
            assert texturecount == self.readInt(), self._err(
                'Expected tex count again') # XXX
            assert facecount == self.readInt(), self._err(
                'Expected face count again') # XXX

            textureCoords = []
            for i in range(texturecount):
                textureCoords.append(self.readPoint())
            channel._textureCoords = tuple(textureCoords)

            if texturecount:
                textureFaces = []
                for i in range(facecount):
                    textureFaces.append(self.readIndexSet())
                channel._textureFaces = tuple(textureFaces)

objectImporters[2] = GeometricObjectNodeImporter


#
# Camera Node import
#
class CameraImporter(TransformableNodeImporter):
    def readObjectNode(self, objectList, flags, length):
        # Import solely based on one camera node in all RA2 files..
        assert flags == 2, self._err('Light Node flags not 3')
        start = self._fd.tell()

        camera = objectList._createObject('Camera')
        camera._name = self.readString()
        camera._tm = self.readTMNode(camera.getName())
        if (self.readInt()):
            camera._target_tm = self.readTMNode()
        camera._type = self.readInt()
        camera._hither = self.readFloat()
        camera._yon = self.readFloat()
        camera._near = self.readFloat()
        camera._far = self.readFloat()
        camera._fov = self.readFloat()
        camera._tdist = self.readFloat()

        # No use, seems to be 4 bytes too short
        # assert self._fd.tell() == start + length, self._err(
        #    'Unexpected Zope node length mismatch')

objectImporters[4] = CameraImporter


#
# Light Node import
#
class LightImporter(TransformableNodeImporter):
    def readObjectNode(self, objectList, flags, length):
        assert flags == 3, self._err('Light Node flags not 3')
        start = self._fd.tell()

        light = objectList._createObject('Light')
        light._name = self.readString()
        light._tm = self.readTMNode(light.getName())

        # These fields are rather speculative; not enough variation in RA2
        # files to be sure; educated guesses from ASE export code.
        light._target = self.readString() 
        if light.getTarget():
            light._target_tm = self.readTMNode(light.getTarget)

        light._type = self.readInt()
        light._shadows = self.readInt()
        light._use_light = self.readInt()
        light._shape = 0 # Doesn't appear to be exported to GMI
        light._color = self.readColor()
        light._intensity = self.readFloat()
        light._aspect = self.readFloat()

        light._unknown = unpack('BBBBBBBB', self._read(8))

        light._attn_start = self.readFloat()
        light._attn_end = self.readFloat()
        light._tdist = self.readFloat()
        light._use_far_attn = self.readByte()
        light._unknown += unpack('BBB', self._read(3))

        # No use checking; seems to be about 6 bytes short.
        # assert self._fd.tell() == start + length, self._err(
        #     'Unexpected Light node length mismatch')
        
objectImporters[5] = LightImporter


#
# Attachment Point
#
class AttachmentPointImporter(TransformableNodeImporter):
    def readObjectNode(self, objectList, flags, length):
        assert flags == 2, self._err('Attachment Point Node flags not 2')
        start = self._fd.tell()
        
        point = objectList._createObject('AttachmentPoint')
        point._name = self.readString()
        point._tm = self.readTMNode(point.getName())
        point._setUserData(self.readString())

        assert self._fd.tell() == start + length, self._err(
            'Unexpected Attachment Point node length mismatch')
    
objectImporters[21] = AttachmentPointImporter


#
# Simulation Object import
#
class SimulationObjectImporter(BinaryImporter):
    def readObjectNode(self, objectList, flags, length):
        assert flags == 2, self._err('Simulation Object Node flags not 2')
        start = self._fd.tell()

        sim = objectList._createObject('SimulationObject')
        sim._name = self.readString()
        sim._gravity = self.readVector()
        sim._worldscale = self.readFloat()
        sim._simtolerance = self.readFloat()
        sim._resolver = self.readInt()
        sim._incl_drag = self.readByte()
        sim._linear_drag = self.readFloat()
        sim._angular_drag = self.readFloat()
        sim._incl_deactivator = self.readByte()
        sim._shortfreq = self.readFloat()
        sim._longfreq = self.readFloat()
        sim._fast_subspace = self.readByte()
        sim._updates_per_timestep = self.readFloat()
        sim._coll_pairs = self.readInt()

        # No point, seems to be about 4 bytes short
        # assert self._fd.tell() == start + length, self._err(
        #     'Unexpected Simulation Object node length mismatch')

objectImporters[30] = SimulationObjectImporter


#
# Constraint Solver node import
#
class ConstraintSolverImporter(TransformableNodeImporter):
    def readObjectNode(self, objectList, flags, length):
        assert flags == 3, self._err('Constraint Solver Node flags not 3')
        start = self._fd.tell()

        solver = objectList._createObject('ConstraintSolver')
        solver._name = self.readString()
        solver._treshold = self.readFloat()
        solver._rb_coll = self.readString()
        
        constraints = self.readInt()
        if constraints:
            self.readConstraintList(constraints, solver._constraints)

        # No point, no length given
        # assert self._fd.tell() == start + length, self._err(
        #     'Unexpected Constraint Solver node length mismatch')

    constraintImporters = {}
    def readConstraintList(self, sizehint, constraintList):
        id, flags, length = self.readTriplet()
        assert id == 44, self._err('Expected Constraint List node')
        assert flags == 2, self._err('Constraint List Node flags not 2')
        start = self._fd.tell()

        count = self.readInt()
        assert count == sizehint, self._err(
            'Inconsistent ConstraintList item count')

        for i in range(count):
            node_id, node_flags, subnode_length = self.readTriplet()
            importer = getattr(self,
               self.constraintImporters.get(node_id, '_unknown'), None)
            if not importer:
                raise ImportException, self._err('Unknown contraint node type')
            importer(constraintList, node_flags, subnode_length)

        # No point, no length given
        # assert self._fd.tell() == start + length, self._err(
        #     'Unexpected Constraint List node length mismatch')

    constraintImporters[46] = 'readPivotConstraint'
    def readPivotConstraint(self, constraintList, flags, lenght):
        assert flags == 2, self._err('Pivot Constraint Node flags not 2')
        start = self._fd.tell()

        hinge = constraintList._createConstraint('PivotConstraint')
        hinge._name = self.readString()
        hinge._tm = self.readTMNode(hinge.getName())
        hinge._body1 = self.readString()
        hinge._body2 = self.readString()
        hinge._point = self.readPoint()
        hinge._spin_axis = self.readVector()
        hinge._unknown = (self.readFloat(), self.readFloat(), self.readFloat(), 
                          self.readFloat(), self.readFloat(), self.readFloat(), 
                          self.readFloat(), self.readFloat())

        # No point, no length given
        # assert self._fd.tell() == start + length, self._err(
        #     'Unexpected Constraint List node length mismatch')

    constraintImporters[47] = 'readHingeConstraint'
    def readHingeConstraint(self, constraintList, flags, lenght):
        assert flags == 2, self._err('Hinge Constraint Node flags not 2')
        start = self._fd.tell()

        hinge = constraintList._createConstraint('HingeConstraint')
        hinge._name = self.readString()
        hinge._tm = self.readTMNode(hinge.getName())
        hinge._body1 = self.readString()
        hinge._body2 = self.readString()
        hinge._point = self.readPoint()
        hinge._spin_axis = self.readVector()
        hinge._is_limited = self.readByte()
        hinge._friction = self.readFloat()
        hinge._angle_limits = (self.readFloat(), self.readFloat())

        # No point, no length given
        # assert self._fd.tell() == start + length, self._err(
        #     'Unexpected Constraint List node length mismatch')

    constraintImporters[53] = 'readPointToPointConstraint'
    def readPointToPointConstraint(self, constraintList, flags, lenght):
        assert flags == 2, self._err(
            'Point-to-point Constraint Node flags not 2')
        start = self._fd.tell()

        hinge = constraintList._createConstraint('PointToPointConstraint')
        hinge._name = self.readString()
        hinge._tm = self.readTMNode(hinge.getName())
        hinge._body1 = self.readString()
        hinge._body2 = self.readString()
        hinge._point1 = self.readPoint()
        hinge._point2 = self.readPoint()

        # No point, no length given
        # assert self._fd.tell() == start + length, self._err(
        #     'Unexpected Constraint List node length mismatch')

objectImporters[42] = ConstraintSolverImporter


#
# Angular Dashpot Importer
#
class AngularDashpotImporter(TransformableNodeImporter):
    def readObjectNode(self, objectList, flags, length):
        assert flags == 2, self._err('Angular Dashpot Node flags not 2')
        start = self._fd.tell()

        adashpot = objectList._createObject('AngularDashpot')
        adashpot._name = self.readString()
        adashpot._tm = self.readTMNode(adashpot.getName())
        adashpot._body1 = self.readString()
        adashpot._body2 = self.readString()
        adashpot._strength = self.readFloat()
        adashpot._damping = self.readFloat()
        adashpot._allow_interpenetrations = self.readByte()
        adashpot._quad = (self.readFloat(), self.readFloat(), self.readFloat(), 
                          self.readFloat())

        # No point, no length given
        # assert self._fd.tell() == start + length, self._err(
        #     'Unexpected Angular Dashpot node length mismatch')

objectImporters[49] = AngularDashpotImporter


#
# Rigid Body Collection importer
#
class RigidBodyCollectionImporter(TransformableNodeImporter):
    def readObjectNode(self, objectList, flags, length):
        assert flags == 4, self._err('Rigid Body Collection Node flags not 4')
        start = self._fd.tell()

        rbcollection = objectList._createObject('RigidBodyCollection')
        rbcollection._name = self.readString()
        paircount = self.readInt()
        rbcollection._solver_type = self.readInt()

        self.readRigidBodyList(rbcollection._rigid_bodies)
        if paircount:
            self.readDisabledCollisionPairs(rbcollection, paircount)
    
        # No point, no length given
        # assert self._fd.tell() == start + length, self._err(
        #     'Unexpected Rigid Body Collection node length mismatch')

    def readRigidBodyList(self, rb_list, sizehint=None):
        id, flags, length = self.readTriplet()
        assert id == 33, self._err('Expected Rigid Body List node')
        assert flags == 2, self._err('Rigid Body List Node flags not 2')
        start = self._fd.tell()

        count = self.readInt()
        if sizehint is not None:
            assert sizehint == count, self._err(
                'Inconsistent rigid body list count?')

        for i in range(count):
            self.readRigidBody(rb_list)

        # No point, no length given
        # assert self._fd.tell() == start + length, self._err(
        #     'Unexpected Rigid Body List node length mismatch')

    def readRigidBody(self, rb_list):
        id, flags, length = self.readTriplet()
        assert id == 32, self._err('Expected Rigid Body List node')
        assert flags == 4, self._err('Rigid Body Node flags not 4')
        start = self._fd.tell()

        rbody = rb_list._createRigidBody()
        rbody._name = self.readString()
        rbody._mass = self.readFloat()
        rbody._elasticity = self.readFloat()
        rbody._friction = self.readFloat()
        rbody._opt_lvl = self.readFloat()
        rbody._unyielding = self.readInt()
        rbody._sim_geom = self.readInt()
        rbody._geom_proxy_name = self.readString()
        rbody._use_display_proxy = self.readByte()
        rbody._disable_collisions = self.readByte()
        rbody._inactive = self.readByte()
        rbody._display_proxy_name = self.readString()
        rbody._tm = self.readTMNode(rbody.getName())
        rbody._geo_type = self.readInt()

        childcount = self.readInt()
        if childcount:
            self.readRigidBodyList(rbody._children, childcount)

        # No point, no length given
        # assert self._fd.tell() == start + length, self._err(
        #     'Unexpected Rigid Body node length mismatch')

    def readDisabledCollisionPairs(self, rbcollection, sizehint):
        id, flags, length = self.readTriplet()
        assert id == 51, self._err('Expected Disabled Collision Pairs node')
        assert flags == 2, self._err(
            'Disabled Collision Pairs Node flags not 2')
        start = self._fd.tell()

        count = self.readInt()
        assert sizehint == count, self._err(
            'Inconsistent count for Disabled Collision Pairs?')

        pairs = []
        for i in range(count):
            pairs.append((self.readString(), self.readString()))

        rbcollection._disabled_collision_pairs = tuple(pairs)

        # No point, no length given
        # assert self._fd.tell() == start + length, self._err(
        #     'Unexpected Disabled Collision Pairs node length mismatch')

objectImporters[31] = RigidBodyCollectionImporter
