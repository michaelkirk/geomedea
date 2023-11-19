Store geographic data of any size and get it back quickly.

- geometry stored with fixed precision (stored as a scaled integer, not floating point)
  - 32bits, like OSM (or adjustable?)
  - offset encoding
- everything is wgs84, like OSM
- rigid schema, but sparse properties allowed
- random access of features, but the feature itself is processed serially so we can efficiently pack things.
- compression? It's hard to apply transparently when you want to randomly access features.
  - paged approach?

Write once, read many times.
- packed hilbert indexed

This is starting to look a lot like pbf.
Store feature geoms, not nodes/ways/relations

Index node:

Data Types
Same as protobuf?

Int/UInt (stored as varint)
String
Bytes
FixedSizeNumericTypes 

Leaf:
(byte offset to page, index within that page)

Page is a flatbuffer/capnp of many records?
Pages can be compressed

Is offset encoding less useful vis a vis compression?

## TODO

