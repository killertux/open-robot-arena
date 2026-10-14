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

"""Object nodes."""

from NodeList import NodeList

#
# Object node list
#
objectClasses= {}
def registerObjectNodeClass(klass):
    objectClasses[klass.__name__] = klass

class ObjectList(NodeList):
    """All objects in a scene grouped together"""

    def _createObject(self, klass):
        """Add an empty Object Node by object class and return it.

        Used by importer.

        """
        self._items += (objectClasses[klass](),)
        return self._items[-1]

IDENTITY = ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0), (0.0, 0.0, 0.0))

#
# Geometric object
#
class TextureChannel:
    _textureCoords = _textureFaces = ()

    def getTextureCoords(self):
        return self._textureCoords

    def getTextureFaces(self):
        return self._textureFaces


class TextureChannels(NodeList):
    """Texture channels on a mesh"""
    def __init__(self, mesh):
        # At least one channel; data is on the mesh itself.
        self._items = (mesh,)
    
    def _createChannel(self):
        self._items += (TextureChannel(),)
        return self._items[-1]


class GeometricMesh(TextureChannel):
    """Mesh for geometric object"""
    _time = _materialRef = _backface_cull = 0
    _points = _faces = _textureCoords = _textureFaces = _normals = ()
    _colors = _colorFaces = ()

    _textureChannels = None

    def __init__(self):
        self._textureChannels = TextureChannels(self)

    def getTime(self):
        return self._time

    def getMaterialReference(self):
        return self._materialRef

    def getCullBackface(self):
        return self._backface_cull

    def getPoints(self):
        return self._points

    def getFaces(self):
        return self._faces

    def getColors(self):
        return self._colors

    def getColorFaces(self):
        return self._colorFaces

    def getNormals(self):
        return self._normals

    def getTextureChannelCount(self):
        return len(self._textureChannels)

    def getTextureChannel(self, index):
        return self._textureChannels[index]

    def getTextureChannels(self):
        return self._textureChannels


class GeometricObject:
    """Geometric object"""
    _name = _parent = ''
    _shade_verts = 0
    _tm = IDENTITY
    _mesh = None

    def __init__(self):
        self._mesh = GeometricMesh()

    def getName(self):
        return self._name

    def getParent(self):
        return self._parent

    def getShadeVertices(self):
        return self._shade_verts

    def getTransformationMatrix(self):
        return self._tm

    def getMesh(self):
        return self._mesh

registerObjectNodeClass(GeometricObject)


#
# Camera nodes
#
CAM_TYPES = {
    0: 'Target',
    1: 'Free'
}
class Camera:
    _name = ''
    _tm = _target_tm = IDENTITY
    _type = 0
    _hither = _yon = _near = _far = _fov = _tdist = 0.0

    def getName(self):
        return self._name

    def getType(self):
        return CAM_TYPES[self._type]

    def getTransformationMatrix(self):
        return self._tm

    def getTargetTransformationMatrix(self):
        return self._target_tm

    def getHither(self):
        return self._hither

    def getYon(self):
        return self._yon

    def getNear(self):
        return self._near

    def getFar(self):
        return self._far

    def getFieldOfView(self):
        return self._fov

    def getTargetDistance(self):
        return self._tdist

registerObjectNodeClass(Camera)


#
# Light nodes
#
LIGHTTYPES = {
    0: 'Omni',
    1: 'Target',
    2: 'Directional',
    3: 'Free',
}

LIGHTSHADOWS = {
    0: 'Off',
    1: 'Mapped',
    2: 'Raytraced'
}

LIGHTSHAPE = {
    0: 'Circle',
    1: 'Rect'
}

class Light:
    _name = _target = ''
    _tm = _target_tm = IDENTITY
    _color = (0.0, 0.0, 0.0)
    _type = _shadows = _use_light = _shape = _use_far_attn = 0
    _intensity = _aspect = _attn_start = _attn_end = _tdist = 0.0

    def getName(self):
        return self._name

    def getTransformationMatrix(self):
        return self._tm

    def getTarget(self):
        return self._target

    def getTargetTransformationMatrix(self):
        return self._target_tm

    def getType(self):
        return LIGHTTYPES[self._type]

    def getShadows(self):
        return LIGHTSHADOWS[self._shadows]

    def getUseLight(self):
        return self._use_light

    def getShape(self):
        return LIGHTSHAPE[self._shape]

    def getColor(self):
        return self._color

    def getIntensity(self):
        return self._intensity

    def getAspect(self):
        return self._aspect

    def getAttenuationStart(self):
        return self._attn_start

    def getAttenuationEnd(self):
        return self._attn_end

    def getTargetDistance(self):
        return self._tdist

    def getUseFarAttenuation(self):
        return self._use_far_attn

registerObjectNodeClass(Light)

#
# Attachment Point (GMI extension)
#
class AttachmentPoint:
    _name = 0 
    _tm = IDENTITY
    _data = None

    def __init__(self):
        self._data = {}

    def __getattr__(self, key):
        try:
            return self._data[key]
        except KeyError:
            raise AttributeError, key

    def _setUserData(self, data):
        """Split out userdata string into a dict."""
        for line in data.splitlines():
            if not line.strip(): continue
            field, val = line.split('=', 1)
            self._data[field.strip().lower()] = val.strip().lower()
            self._cleanData()

    def _cleanData(self):
        for key, val in self._data.items():
            if val == '<None>':
                self._data[key] = None
            if val.find(',') > -1 and val.find('=') > -1:
                val, data = val.split(',', 1)
                self._data[key] = val.strip()
                field, val = data.split('=', 1)
                self._data[field.strip().lower()] = val.strip().lower()
                return self._cleanData()

    def getName(self):
        return self._name

    def getTransformationMatrix(self):
        return self._tm

registerObjectNodeClass(AttachmentPoint)


#
# Havok (3D and simulation engine) Simulation Object
#
class SimulationObject:
    _name = ''
    _gravity = (0.0, 0.0, 0.0)
    _worldscale = _simtolerance = _linear_drag = _angular_drag = 0.0
    _shortfreq = _longfreq = _updates_per_timestep = 0.0
    _resolver = _incl_drag = _incl_deactivator = _fast_subspace = 0
    _coll_pairs = 0 # speculation: possibly this object contains a set of pairs

registerObjectNodeClass(SimulationObject)


#
# Havok Constraint Solver
#
constraintClasses= {}
def registerConstraintClass(klass):
    constraintClasses[klass.__name__] = klass

class ConstraintsList(NodeList):
    """Constraints in a constraint solver"""
    def _createConstraint(self, klass):
        self._items += (constraintClasses[klass](),)
        return self._items[-1]


class BaseConstraint:
    _name = _body1 = _body2 = ''
    _tm = IDENTITY

    def getName(self):
        return self._name


class PivotConstraint(BaseConstraint): 
    # Name and fields are guess; used by car steering components
    _point = _spin_axis = (0.0, 0.0, 0.0)
    _unknown = (0.0, 0.0, 0.0, 0.0, 
                0.0, 0.0, 0.0, 0.0)

registerConstraintClass(PivotConstraint)


class HingeConstraint(BaseConstraint):
    _point = _spin_axis = (0.0, 0.0, 0.0)
    _is_limited = 0
    _friction = 0.0
    _angle_limits = (0.0, 0.0)

registerConstraintClass(HingeConstraint)


class PointToPointConstraint(BaseConstraint):
    _point1 = _point2 = (0.0, 0.0, 0.0)

registerConstraintClass(PointToPointConstraint)


class ConstraintSolver:
    _name = ''
    _treshold = 0.0
    _rb_coll = ''

    _constraints = None

    def __init__(self):
        self._constraints = ConstraintsList()

    def getConstraintCount(self):
        return len(self._constraints)

    def getConstraint(self, index):
        return self._constraints[index]

    def getConstraints(self):
        return self._constraints

registerObjectNodeClass(ConstraintSolver)


#
# Havok Angular Dashpot
#
class AngularDashpot:
    _name = _body1 = _body2 = ''
    _tm = IDENTITY
    _strength = _damping = 0.0
    _allow_interpenetrations = 0
    _quad = (0.0, 0.0, 0.0, 0.0)

    def getName(self):
        return self._name

registerObjectNodeClass(AngularDashpot)


#
# Rigid Body collection
#
class RigidBody:
    _name = _geom_proxy_name = _display_proxy_name = ''
    _mass = _elasticity = _friction = _opt_lvl = 0.0 
    _unyielding = _sim_geom = _use_display_proxy = _disable_collisions = 0
    _inactive = _geo_type = 0
    _tm = IDENTITY

    _children = None

    def __init__(self):
        self._children = RigidBodyList()
    
    def getName(self):
        return self._name

    def getTransformationMatrix(self):
        return self._tm

    def getGeometricProxyName(self):
        return self._geom_proxy_name

    def getDisplayProxyName(self):
        return self._display_proxy_name

    def getMass(self):
        return self._mass

    def getElasticity(self):
        return self._elasticity

    def getFriction(self):
        return self._friction
    
    def getOptimizationLevel(self):
        return self._opt_lvl

    def getUnyielding(self):
        return self._unyielding

    def getSimulationGeometry(self):
        return self._sim_geom

    def getUseDisplayProxy(self):
        return self._use_display_proxy
    
    def getDisableCollisions(self):
        return self._disable_collisions

    def getInactive(self):
        return self._inactive

    def getChildCount(self):
        return len(self._children)

    def getChild(self, index):
        return self._children[index]

    def getChildren(self):
        return self._children


class RigidBodyList(NodeList):
    def _createRigidBody(self):
        self._items += (RigidBody(),)
        return self._items[-1]

class RigidBodyCollection:
    _name = ''
    _solver_type = 0
    _disabled_collision_pairs = ()

    _rigid_bodies = None

    def __init__(self):
        self._rigid_bodies = RigidBodyList()

    def getRigidBodyCount(self):
        return len(self._rigid_bodies)

    def getRigidBody(self, index):
        return self._rigid_bodies[index]

    def getRigidBodies(self):
        return self._rigid_bodies
        
registerObjectNodeClass(RigidBodyCollection)
