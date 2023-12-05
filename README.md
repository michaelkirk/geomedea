geomedea is a geospatial data format optimized for remotely accessing a subset of planet-wide data.

🚨 This project is in an experimental state, but it shows some promise.

This format is heavily inspired by [flatgeobuf](https://github.com/flatgeobuf/flatgeobuf).

Like flatgeobuf, a spatial index (currently a single packed hilbert index) allows for efficiently selecting features within a given bounding box, even remotely via HTTP range requests.

Unlike flatgeobuf, which leaves compression as an "exercise for the reader", compression is built into the format in a way that still allows you to leverage the spatial index.

To achieve that, rather than a single stream of features like flatgeobuf, features are chunked into pages. Each pages is then compressed. When using a bbox query, you fetch only the pages that contain your features. Because pages contain other features, this involves some wastage, but in practice the losses from having to fetching an entire page of features when you only need part of it are typically overshadowed by the gains from being able to utilize compression.

If your experience differs I'd love to know.

## Benefits

Here's a simple benchmark in the repository comparing flatgeobuf's rust client with a geomedea client on a simulated connection with 50mbit w/ 100ms latency.

### FGB - UScounties.fgb 13M

```
HTTP select_all                 time: [3.0314 s 3.0359 s 3.0399 s]
HTTP select_bbox                time: [629.16 ms 631.99 ms 634.83 ms]
```

### Geomedea compressed: USCounties-compressed.geomedea 5.1M

```
HTTP select_all (compressed)    time: [994.97 ms 1.0145 s 1.0428 s]
HTTP select_bbox (compressed)   time: [529.22 ms 530.04 ms 530.85 ms]
```

### Geomedea uncompressed: USCounties-uncompressed.geomedea 7.5M

```
HTTP select_all (uncompressed)  time: [1.4355 s 1.4405 s 1.4445 s]
HTTP select_bbox (uncompressed) time: [630.31 ms 632.56 ms 634.58 ms]
```

## Caveats

The performance is almost entirely network dominated - both round trip latency and throughput play a role. The index format and traversal logic is largely the same between formats.

The performance gains are largely due to breaking features into compressible pages. I've made several other conceptually smaller, but still significant, changes.

### More space efficient encoding

With flatbuffers, it's possible to randomly access fields within your flatbuffer without having to parse everything before it. This is in contrast to formats like protobuf which must be accessed serially. There are some clever tricks required to achieve this, but one component is simply adding padding to predicatly align fields. In exchange for this "wasted" space you get this nice random access feature.

Flatgeobuf uses flatbuffers to encode its features. Features consist of geometry data and property data. Geometry encoding is pretty straight forward - a bunch of 64 bit floats in a flatbuffer vector. Property data on the other hand has its own custom binary encoding on top of a flatbuffer byte array - so you can't randomly access a property of the feature without first processing every proceeding property.

Further, and maybe more significantly, each feature in a flatgeobuf is serialized as an independent size-prefixed buffer. Given just the feature stream, you can't get to the _nth_ feature without first parsing the preceeding _n - 1_ features.

With flatgeobuf's spatial index we _can_ randomly access features, but that's by virtue of the external index, and has nothing to due with the flatbuffer encoding.

So if it's the hilbert index that gets us random access to features, and the properties are also not randomly accessible. There may be use cases where it's significantly useful to be able to skip over *all* the property data or accessing the geometry coordinates without parsing them, but in my own use cases the benefit of the flatbuffer encoding is dwarfed by the overhead of the padding.

So instead of flatbuffers, I'm using a space efficient serialized encoding. Currently that's bincode, which was easy for me to use and performs decently, but I didn't think very hard about that choice. There may be a better choice, especially in terms of cross platform support.

With compression available, the padding of flatbuffers becomes less of an issue, so the overall benefit here is actually not as much as it might at first seem. I'm actually considering coming full circle and using some kind of random access format like flatbuffer or capnproto for the feature encoding. One difference I would make, rather than having a series of size prefixed root flatbuffers, would be to have a single root per page - a vec of features, allowing efficient random access to the n'th feature within a page. When flatgebuf was created, flatbuffer didn't support large enough vectors to put a large dataset into a single vec. That's changed with flatbuffers now, but regardless, with the features broken into pages, the count per page would never be very large anyway. Using a random access friendly format within a page might make the indexing strategy more straight forward and accessing only specific properties more efficient.

### Coordinate storage

Whereas flatgebuf stores geometry coordinates as 64-bit floating point numbers, geomedea currently stores coordinates as 32-bit signed integers representing a scaled decimal, just like OpenStreetMap's native coordinate format. This made sense for me since I'm primarily working with OSM data and almost always working with lng/lat rather than some local projection. For these cases, 32bit scaled decimals get you within 1cm at the equator, which is plenty good enough for my use cases. Flatgeobuf isn't "wrong" here, it's just a different choice.

The details of coordinate storage might change, or might become configurable. In practice, for the datasets I was using, scaling up to 64-bit floating point didn't actually impact the compressed size as much as I expected.

An extension to the integer encoding I'd be excited to try would be an offset encoding like the OSM pbf format.
Since the feature pages are in hilbert ordering, subsequent features should be relatively nearby, and thus suitable for efficient offset encoding.

### HTTP Streaming

To limit memory usage, the FGB client uses a "chunked" approach to fetch a fixed amount of data at a time (currently 1MB by default). Once the 1MB is downloaded it's processed and then another chunk is downloaded.
Unlike flatgeobuf's rust client, geomedea uses a streaming http client.
A smaller benefit of this it that it reduces the amount of time that processing and I/O (downloading) are waiting on each other.
More significantly, without a "chunk size" limit on our requests, we no longer need to "split" large sequential ranges into multiple requests.
You can see the benefits of this with the select_all case in particular, which can now be a single request for the entire file. Because we're streaming, we only keep a little in memory at a time, regardless of the overall file size.
There isn't anything inherent in the format that makes streaming easier - this approach could be applied to flatgeobuf. But this being a new code base, I wanted to take advantage of it from the start.
