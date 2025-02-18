**geomedea is a geospatial data format optimized for remotely accessing a subset of planet-wide data.**

🚨 This project is in an experimental state, but it shows some promise.

See the [online demo](https://michaelkirk.github.io/geomedea/examples/maplibre/filtered.html) for a quick example of what geomedea can do.

## Quickstart

Since the format is still undergoing major changes, there is currently only one client. It's written in rust.

You'll need a reasonably up to date rust toolchain - see https://rustup.rs for instructions on installing rust and cargo, the rust package manager.

You can use the geomedea API in this repository directly,
but if you want something ready to use from the command line,
I'd recommend leveraging my integration with the geozero CLI.

To install it globally:
```
cargo install --git https://github.com/michaelkirk/geozero.git --branch mkirk/geomedea-integration
```

See usage:
```
geozero --help
```

Examples
```
# convert my dataset to geomedea
geozero my-data.{geojson,fgb,csv,etc} my-data.geomedea

# convert geomedea to some other output
geozero my-data.geomedea my-data.{geojson,fgb,csv,etc}

# extract only a subset of a geomedea file, from approximately Paris to Brussels
# The output extension can be any format supported by geozero
geozero --extent="2.2,48.7,4.5,50.9" my-data.geomedea my-extract.{geomedea,geojson,fgb,csv,etc}

# geomedea files (like flatgeobuf) may be accessed remotely using HTTP range requests
geozero --extent="2.2,48.7,4.5,50.9" https://my-host.com/my-data.geomedea my-extract.{geomedea,geojson,fgb,csv,etc}

# To fetch the entire file and convert it, omit the `extent` argument
geozero https://my-host.com/my-data.geomedea my-extract.{geojson,fgb,csv,etc}
```

## What is this?

This is a way of encoding spatial data heavily inspired by the excellent [flatgeobuf](https://github.com/flatgeobuf/flatgeobuf) project, but tuned for my specific use cases.

Like flatgeobuf, a spatial index (currently a single packed hilbert index) allows for efficiently selecting features within a given bounding box, even remotely via HTTP range requests.

Unlike flatgeobuf, which leaves compression as an "exercise for the reader", compression (currently zstd) is built into the format in a way that still allows you to leverage the spatial index.

To achieve that, rather than a single stream of features like flatgeobuf, features are chunked into pages.
Each page is then compressed.
When using a bbox query, you fetch only the pages that contain your features.
Because pages contain multiple features, this means you have to fetch an entire page even if you only need one feature in that page.
This wastage is typically outweighed by the gains of being able to utilize compression.

I'd love to know the details if your experience differs.

## Motivation / Measurements

The specific problem I was thinking about when designing this format was wanting to download subsets of the [OpenAddresses](https://openaddresses.io) data bounded by a specified bounding box to feed into non-global deployments of [Headway](https://github.com/headwaymaps/headway) like https://seattle.maps.earth.

Through the OpenAddresses website you can click around to browse through and download a pre-sliced region, but it might not match exactly what you want, and there is no programmatic API for this — it's a process of manual discovery.

So, restating the problem a little more precisely, given a remotely hosted (via HTTP) 1.2G compressed geojson file of global address data from OpenAddresses,
I wanted only those entries within a given bounds (for example, Seattle: -122.462 47.394 -122.005 47.831).
That's about a million records.

My first solution was to convert the entire global dataset into a single flatgeobuf and utilize FGB's bounding box query functionality to download just the subset of data I was interested in.
This worked as expected, but I was dismayed by two things:

Firstly, the size of the flatgeobuf on disk is much larger than the compressed global geojson.
I'm not surprised that it's larger — for one, it contains an index, so there's inherently more information.
Probably more importantly, the geojson is compressed.
You could compress the FGB, but then you lose the ability to do bbox queries on it, because the index is based on (uncompressed) byte offsets into the file.
(*☝️: maybe [SOZip](https://sozip.org) could be helpful here — but I haven't seen it used in an over-the-network context yet.*)

My second big concern was network transfer.
Ultimately I needed this data as a CSV.
The million records in CSV format was 106MB, so I expected (assumed? hoped?) my transfer would be somewhere in the order of 100MB.
However, the amount of data transferred to get the feature data from the flatgeobuf was almost 3.2x larger, which doesn't sit right with me.
We should be able to beat CSV in 2023, right? 😉
This is largely a consequence of my previous point about uncompressed flatgeobuf not being a very space efficient format, but whereas the size of the entire flatgeobuf on disk affects the person hosting the file, this point affects the downloader as well.
I've done some work to optimize network transfer in flatgeobuf over the years to address this,
in particular [improved index traversal](https://github.com/flatgeobuf/flatgeobuf/pull/93) and [smart feature batching](https://github.com/flatgeobuf/flatgeobuf/pull/319),
but I think at this point I've stretched flatgeobuf as far as it can go without breaking.

Here are the numbers:

**FGB**

- Size of input converted to flatgeobuf: 9.3G (7.75X bigger than zipped geojson)
- Example request for Seattle, with simulated network latency 100ms, 50mbps
  - Time: 1:14.30 total
  - Number of requests: 310
  - Bytes transferred: 339838864 (340 MB) (3.2X bigger than output CSV)

**Geomedea**

- Size of input converted to geomedea: 2.5G (2.1X bigger than zipped geojson)
- Example request for Seattle, with simulated network latency 100ms, 50mbps
  - Time: 45.564 total
  - Number of requests: 153
  - Bytes transferred: 81175859 (81MB) (0.76X bigger than output CSV)

## Benefits

A smaller benchmark exists in the repository which you can run to compare to a [similar benchmark](https://github.com/flatgeobuf/flatgeobuf/blob/master/src/rust/benches/http_read.rs) in flatgeobuf.
It runs against a simulated connection with 50mbps w/ 100ms latency.

**FGB - UScounties.fgb 13M**

```
HTTP select_all                 time: [3.0314 s 3.0359 s 3.0399 s]
HTTP select_bbox                time: [629.16 ms 631.99 ms 634.83 ms]
```

**Geomedea compressed: USCounties-compressed.geomedea 5.1M**

```
HTTP select_all (compressed)    time: [994.97 ms 1.0145 s 1.0428 s]
HTTP select_bbox (compressed)   time: [529.22 ms 530.04 ms 530.85 ms]
```

**Geomedea uncompressed: USCounties-uncompressed.geomedea 7.5M**

```
HTTP select_all (uncompressed)  time: [1.4355 s 1.4405 s 1.4445 s]
HTTP select_bbox (uncompressed) time: [630.31 ms 632.56 ms 634.58 ms]
```

## Caveats and details

In real world use cases, the performance is almost entirely network dominated — both round trip latency and throughput play a role.
In this regard, the index format and traversal logic is largely the same between formats.
The biggest conceptual change responsible for the improvement is the breaking up of features into compressible pages, but I've also made several other conceptually smaller, but still significant, changes.

### More space efficient encoding

Flatgeobuf uses flatbuffers to encode its features (hence its name!).

With the flatbuffer format, it's possible to randomly access fields within your flatbuffer without having to parse everything before it. This is in contrast to formats like protobuf which must be accessed serially. There are some clever tricks required to achieve this, but one component is simply adding padding to predictably align fields. In exchange for this "wasted" padding space you gain this nice random access feature.

Features consist of geometry data and property data. Geometry encoding in flatgeobuf is pretty straight forward — a bunch of 64 bit floats in a flatbuffer vector. Property data on the other hand has its own custom binary encoding on top of a flatbuffer byte array — so you can't randomly access a property of the feature without first processing every proceeding property.

Further, and maybe more significantly, each feature in a flatgeobuf is serialized as an independent size-prefixed buffer. Given just the feature portion of the file, you can't get to the _nth_ feature without first parsing the proceeding _n - 1_ features.

Using flatgeobuf's spatial index we _could_ randomly access features, but that's by virtue of the external index, and has nothing to due with the flatbuffer encoding.

So if it's the index that gets us random access to features, and the properties are also not randomly accessible, what are we gaining from the flatbuffer encoding? There may be use cases where it's significantly useful to be able to skip over *all* the property data or accessing some of the geometry coordinates without parsing all of them, but in my own use cases, the benefit of the flatbuffer encoding is dwarfed by its overhead.
So instead of flatbuffers, I'm using a space efficient serialized encoding.
Currently that's [bincode](https://crates.io/crates/bincode), which was easy for me to use and performs well, but I honestly didn't think very hard about this choice.
There may be a better options, especially in terms of cross language support.

#### Future work 🤔

Ironically, with compression available, the padding of flatbuffers becomes less of an issue, so the overall benefit of a more efficient encoding scheme is actually not as much as it might at first seem.
Because of this, I've even considered coming full circle and using some kind of random access format like flatbuffer or capnproto for the feature encoding.
One difference I would consider making, rather than having a series of size prefixed root flatbuffers, would be to have a single root per page — a vec of features rather than a series of size-prefixed features, allowing efficient random access to the n'th feature within a page.
When flatgebuf was created, flatbuffer didn't support large enough vectors to put a large dataset into a single vec.
That's changed with flatbuffers now, but regardless, with the features broken into pages, the count per page would never be very large anyway.
Using a random access friendly format within a page might make the indexing strategy more straight forward and accessing only specific properties more efficient.

### Coordinate storage

Whereas flatgebuf stores geometry coordinates as 64-bit floating point numbers, geomedea currently stores coordinates as 32-bit signed integers representing a scaled decimal, just like OpenStreetMap's native coordinate format.
This made sense for me since I'm primarily working with OSM data and almost always working with lng/lat rather than some local projection.
For these cases, 32-bit scaled decimals get you within about 1cm at the equator, which is much more accurate than most GPS measurements and plenty good enough for my use cases.
Flatgeobuf isn't "wrong" here, it's just a different choice.

#### Future work 🤔

The details of coordinate storage might change, or might become configurable. In practice, for the datasets I was using, scaling up to 64-bit floating point didn't actually hurt the compressed size as much as I expected.

An extension to the integer encoding I'd be excited to try would be an offset encoding like the OSM pbf format.
Since the feature pages are in hilbert ordering, subsequent features should be relatively nearby, and thus suitable for efficient offset encoding.

### HTTP Streaming

To limit memory usage, the FGB client uses a "chunked" approach to fetch a fixed amount of data at a time (currently 1MB by default).
Once the 1MB is downloaded it's processed and then another chunk is downloaded.
Unlike flatgeobuf's rust client, geomedea uses a streaming http client.
A smaller benefit of this it that it reduces the amount of time that processing and I/O (downloading) are waiting on each other.
More significantly, without a "chunk size" limit on our requests, we no longer need to "split" large sequential ranges into multiple requests.
You can most dramatically see the benefits of this with the select_all case, which can now be a single request for the entire file.
Because we're streaming, we only keep a little in memory at a time, regardless of the overall file size.
There isn't anything inherent in the format that makes streaming easier — this approach could be applied to flatgeobuf.
But this being a new code base, I wanted to take advantage of it from the start.
