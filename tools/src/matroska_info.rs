extern crate av_data;
extern crate av_format;
extern crate circular;
extern crate matroska;
extern crate nom;
extern crate pretty_env_logger;

use circular::Buffer;
use nom::{Err, Offset};
use std::env;
use std::fs::File;
use std::io::Read;

use matroska::ebml::ebml_header;
use matroska::elements::{segment, segment_element, SegmentElement};

fn main() {
    pretty_env_logger::init();
    let mut args = env::args();
    let _ = args.next().expect("first arg is program path");
    let filename = args.next().expect("expected file path");

    //println!("filename: {}", filename);
    run(&filename).expect("should parse file correctly");
}

fn run(filename: &str) -> std::io::Result<()> {
    let mut file = File::open(filename)?;

    let capacity = 5242880;
    let mut b = Buffer::with_capacity(capacity);

    // we write into the `&mut[u8]` returned by `space()`
    let sz = file.read(b.space()).expect("should write");
    b.fill(sz);
    //eprintln!("write {:#?}", sz);

    let length = {
        let res = ebml_header(b.data());
        if let Ok((remaining, header)) = res {
            //eprintln!("parsed header: {:#?}", h);
            println!("+ EBML head");
            println!("|+ EBML version: {}", header.version);
            println!("|+ EBML read version: {}", header.read_version);
            println!("|+ EBML maximum ID length: {}", header.max_id_length);
            println!("|+ EBML maximum size length: {}", header.max_size_length);
            println!("|+ Doc type: {}", header.doc_type);
            println!("|+ Doc type read version: {}", header.doc_type_read_version);

            b.data().offset(remaining)
        } else {
            panic!("couldn't parse header");
        }
    };

    let mut _consumed = length;
    b.consume(length);

    let length = {
        let res = segment(b.data());
        if let Ok((remaining, segment)) = res {
            //eprintln!("parsed segment: {:#?}", h);
            println!("+ Segment, size {}", segment.1.unwrap_or(0));

            b.data().offset(remaining)
        } else {
            panic!("couldn't parse header");
        }
    };

    b.consume(length);

    // handle first elements
    let mut seek_head = None;
    let mut info = None;
    let mut tracks = None;

    loop {
        if seek_head.is_some() && info.is_some() && tracks.is_some() {
            break;
        }

        if b.available_space() == 0 {
            b.shift();
            if b.available_space() == 0 {
                println!("buffer is already full,  cannot refill");
                break;
            }
        }

        // refill the buffer
        let sz = file.read(b.space()).expect("should read");
        b.fill(sz);

        // if there's no more available data in the buffer after a write, that means we reached
        // the end of the file
        if b.available_data() == 0 {
            panic!("no more data to read or parse, stopping the reading loop");
        }

        let offset = {
            let (i, element) = match segment_element(b.data()) {
                Ok((i, o)) => (i, o),
                Err(Err::Error(e)) | Err(Err::Failure(e)) => panic!("failed parsing: {:?}", e),
                Err(Err::Incomplete(_i)) => continue,
            };

            match element {
                SegmentElement::SeekHead(s) => {
                    println!("|+ Seek head (size {}) {} elements", b.data().offset(i), s.positions.len());
                    if seek_head.is_some() {
                        panic!("already got a SeekHead element");
                    } else {
                        seek_head = Some(s);
                    }
                }
                SegmentElement::Info(i) => {
                    println!("|+ Segment Information");
                    println!("| + timestamp scale: {}", i.timecode_scale);
                    println!("| + multiplexing application: {}", i.muxing_app);
                    println!("| + writing application: {}", i.writing_app);
                    println!(
                        "| + segment UID: {:?}",
                        i.segment_uid.as_ref().unwrap_or(&Vec::new())
                    );
                    println!("| + duration: {}s", i.duration.unwrap_or(0f64) / 1000f64);
                    if info.is_some() {
                        panic!("already got an Info element");
                    } else {
                        info = Some(i);
                    }
                }
                SegmentElement::Tracks(t) => {
                    //eprintln!("got tracks: {:#?}", t);
                    println!("|+ Segment tracks");
                    for tr in t.tracks.iter() {
                        println!("| + A track");
                        println!("|  + Track number: {}", tr.track_number);
                        println!("|  + Track UID: {}", tr.track_uid);
                        println!("|  + Track type: {}", tr.track_type);
                        println!("|  + Lacing flag: {}", tr.flag_lacing.unwrap_or(0));
                        println!("|  + Default flag: {}", tr.flag_default.unwrap_or(0));
                        println!(
                            "|  + Language: {}",
                            tr.language.as_ref().unwrap_or(&"".to_string())
                        );
                        println!("|  + Codec ID: {}", tr.codec_id);
                        println!(
                            "|  + Codec private: length {}",
                            tr.codec_private.as_ref().map(|v| v.len()).unwrap_or(0)
                        );

                        if let Some(ref v) = tr.video {
                            println!("|  + Video track");
                            println!("|    + Pixel width: {}", v.pixel_width);
                            println!("|    + Pixel height: {}", v.pixel_height);
                            if let Some(inter) = v.flag_interlaced {
                                println!("|    + Interlaced: {}", inter);
                            }
                            if let Some(width) = v.display_width {
                                println!("|    + Display width: {}", width);
                            }
                            if let Some(height) = v.display_height {
                                println!("|    + Display height: {}", height);
                            }
                            if let Some(unit) = v.display_unit {
                                println!("|    + Display unit: {}", unit);
                            }
                        }

                        if let Some(ref a) = tr.audio {
                            println!("|  + Audio track");
                            println!("|    + Sampling frequency: {}", a.sampling_frequency);
                            if let Some(frequency) = a.output_sampling_frequency {
                                println!("|    + Output sampling freqeuncy: {}", frequency);
                            }
                            println!("|    + Channels: {}", a.channels);
                            if let Some(ref channel_positions) = a.channel_positions {
                                println!("|    + Channel position: {:?}", channel_positions);
                            }
                            if let Some(bit_depth) = a.bit_depth {
                                println!("|    + Bit depth: {}", bit_depth);
                            }
                        }
                    }
                    if tracks.is_some() {
                        panic!("already got a Tracks element");
                    } else {
                        tracks = Some(t);
                    }
                }
                /*SegmentElement::Cluster(c) => {
                    println!("|+ Cluster");
                    eprintln!("got a cluster: {:#?}", c);
                },*/
                SegmentElement::Void(s) => {
                    println!("|+ EbmlVoid (size: {})", s);
                }
                el => {
                    panic!("got unexpected element: {:#?}", el);
                }
            }

            b.data().offset(i)
        };
        _consumed += offset;
        //eprintln!("consumed {} bytes", length);
        b.consume(offset);
    }

    loop {
        if b.available_space() == 0 {
            b.shift();
            if b.available_space() == 0 {
                println!("buffer is already full,  cannot refill");
                break;
            }
        }

        // refill the buffer
        let sz = file.read(b.space()).expect("should read");
        b.fill(sz);

        /*
        eprintln!(
            "refill: {} more bytes, available data: {} bytes, consumed: {} bytes",
            sz,
            b.available_data(),
            consumed
        );
        */

        // if there's no more available data in the buffer after a write, that means we reached
        // the end of the file
        if b.available_data() == 0 {
            //panic!("no more data to read or parse, stopping the reading loop");
            break;
        }

        let offset = {
            let (i, element) = match segment_element(b.data()) {
                Ok((i, o)) => (i, o),
                Err(Err::Error(e)) | Err(Err::Failure(e)) => panic!("failed parsing: {:?}", e),
                Err(Err::Incomplete(_)) => continue,
            };

            match element {
                SegmentElement::SeekHead(_)
                | SegmentElement::Info(_)
                | SegmentElement::Tracks(_) => {
                    panic!(
                        "unexpected seek head, info or tracks element: {:?}",
                        element
                    );
                }
                SegmentElement::Cluster(c) => {
                    println!("|+ Cluster");
                    println!("|+   timecode: {}", c.timecode);
                    println!("|+   silent_tracks: {:?}", c.silent_tracks);
                    println!("|+   position: {:?}", c.position);
                    println!("|+   prev_size: {:?}", c.prev_size);
                    println!("|+   simple_block: {} elements", c.simple_block.len());
                    println!("|+   block_group: {} elements", c.block_group.len());
                    println!(
                        "|+   encrypted_block: {:?} bytes",
                        c.encrypted_block.as_ref().map(|s| s.len())
                    );
                    //eprintln!("got a cluster: {:#?}", c);
                }
                SegmentElement::Void(s) => {
                    println!("|+ EbmlVoid (size: {})", s);
                }
                SegmentElement::Tags(_) => {
                    println!("|+ Tags");
                }
                SegmentElement::Cues(_) => {
                    println!("|+ Cues");
                }
                SegmentElement::Unknown(id, data) => {
                    panic!(
                        "offset {:X?}: got unknown element: {:X?} {:#?}",
                        _consumed, id, data
                    );
                }
                el => {
                    panic!("got unexpected element: {:#?}", el);
                }
            }

            b.data().offset(i)
        };
        _consumed += offset;
        //eprintln!("consumed {} bytes", length);
        b.consume(offset);
    }

    Ok(())
}
