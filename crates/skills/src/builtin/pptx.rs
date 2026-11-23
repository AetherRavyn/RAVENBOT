//! A minimal, valid image-only PPTX package.
//!
//! Each slide is one picture at `(0,0)` sized `12192000 × 6858000` EMU, which
//! is exactly 13.333 × 7.5 inches (16:9). There is no text frame, placeholder,
//! or theme that PowerPoint can reflow: the design is the image. Use
//! `make_document` when the user needs editable slide text.
//!
//! The package is deliberately small rather than a clone of PowerPoint's full
//! part set. It contains what an independent OOXML reader needs: content
//! types, package relationships, a presentation, one master, one blank layout,
//! a theme, one slide part and one media part per image.

use std::io::{Read, Write};
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const NS_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const NS_CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn content_types(images: usize) -> String {
    let mut overrides = String::from(
        r#"<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
<Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
<Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>"#,
    );
    for index in 1..=images {
        overrides.push_str(&format!(
            r#"<Override PartName="/ppt/slides/slide{index}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="{NS_CT}">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Default Extension="png" ContentType="image/png"/>
{overrides}
</Types>"#
    )
}

fn root_rels() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="{NS_REL}">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/>
</Relationships>"#
    )
}

fn presentation(images: usize) -> String {
    let mut slides = String::new();
    for index in 1..=images {
        slides.push_str(&format!(
            r#"<p:sldId id="{id}" r:id="rId{rel}"/>"#,
            id = 255 + index,
            rel = index + 1
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}">
<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
<p:sldIdLst>{slides}</p:sldIdLst>
<p:sldSz cx="12192000" cy="6858000"/>
<p:notesSz cx="6858000" cy="9144000"/>
</p:presentation>"#
    )
}

fn presentation_rels(images: usize) -> String {
    let mut rels = String::from(
        r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>"#,
    );
    for index in 1..=images {
        rels.push_str(&format!(
            r#"<Relationship Id="rId{rel}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{index}.xml"/>"#,
            rel = index + 1
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="{NS_REL}">{rels}</Relationships>"#
    )
}

fn empty_sp_tree() -> String {
    r#"<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree>"#.to_string()
}

fn slide_master() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}">
<p:cSld name="RAVENBOT Master">{sp}</p:cSld>
<p:clrMap accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" bg1="lt1" bg2="lt2" folHlink="folHlink" hlink="hlink" tx1="dk1" tx2="dk2"/>
<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst>
<p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles>
</p:sldMaster>"#,
        sp = empty_sp_tree()
    )
}

fn slide_master_rels() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="{NS_REL}">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/>
</Relationships>"#
    )
}

fn slide_layout() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}" type="blank" preserve="1">
<p:cSld name="Blank">{sp}</p:cSld>
<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sldLayout>"#,
        sp = empty_sp_tree()
    )
}

fn slide_layout_rels() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="{NS_REL}">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>"#
    )
}

fn theme() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="{NS_A}" name="RAVENBOT">
<a:themeElements>
<a:clrScheme name="RAVENBOT">
<a:dk1><a:srgbClr val="111318"/></a:dk1><a:lt1><a:srgbClr val="F8FAFC"/></a:lt1>
<a:dk2><a:srgbClr val="1B1F27"/></a:dk2><a:lt2><a:srgbClr val="E2E8F0"/></a:lt2>
<a:accent1><a:srgbClr val="79B8FF"/></a:accent1><a:accent2><a:srgbClr val="9AE6B4"/></a:accent2>
<a:accent3><a:srgbClr val="FFD591"/></a:accent3><a:accent4><a:srgbClr val="FF96A0"/></a:accent4>
<a:accent5><a:srgbClr val="C4B5FD"/></a:accent5><a:accent6><a:srgbClr val="93DCDA"/></a:accent6>
<a:hlink><a:srgbClr val="79B8FF"/></a:hlink><a:folHlink><a:srgbClr val="A6D2FF"/></a:folHlink>
</a:clrScheme>
<a:fontScheme name="RAVENBOT"><a:majorFont><a:latin typeface="Inter"/></a:majorFont><a:minorFont><a:latin typeface="Inter"/></a:minorFont></a:fontScheme>
<a:fmtScheme name="RAVENBOT">
<a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst>
<a:lnStyleLst><a:ln w="9525"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="25400"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="38100"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst>
<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>
<a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst>
</a:fmtScheme>
</a:themeElements>
</a:theme>"#
    )
}

fn slide(index: usize) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}">
<p:cSld><p:spTree>
<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
<p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>
<p:pic>
<p:nvPicPr><p:cNvPr id="2" name="Slide {index}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr>
<p:blipFill><a:blip r:embed="rId2"/><a:stretch><a:fillRect/></a:stretch></p:blipFill>
<p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="12192000" cy="6858000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>
</p:pic>
</p:spTree></p:cSld>
<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sld>"#
    )
}

fn slide_rels(index: usize) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="{NS_REL}">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image{index}.png"/>
</Relationships>"#
    )
}

fn core_properties(title: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<dc:title>{}</dc:title><dc:creator>RAVENBOT</dc:creator><cp:lastModifiedBy>RAVENBOT</cp:lastModifiedBy>
</cp:coreProperties>"#,
        xml_escape(title)
    )
}

fn app_properties(slides: usize) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">
<Application>RAVENBOT</Application><PresentationFormat>On-screen Show (16:9)</PresentationFormat><Slides>{slides}</Slides>
</Properties>"#
    )
}

/// Write `images` as one full-bleed slide each.
pub fn write_pptx(images: &[&Path], output: &Path, title: &str) -> Result<(), String> {
    if images.is_empty() {
        return Err("A deck needs at least one slide image.".to_string());
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create '{}': {error}", parent.display()))?;
    }
    let file = std::fs::File::create(output)
        .map_err(|error| format!("Could not create '{}': {error}", output.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    fn put(
        zip: &mut ZipWriter<std::fs::File>,
        name: &str,
        body: String,
        options: SimpleFileOptions,
    ) -> Result<(), String> {
        zip.start_file(name, options)
            .map_err(|error| format!("Could not add '{name}' to the PPTX: {error}"))?;
        zip.write_all(body.as_bytes())
            .map_err(|error| format!("Could not write '{name}' into the PPTX: {error}"))
    }

    put(
        &mut zip,
        "[Content_Types].xml",
        content_types(images.len()),
        options,
    )?;
    put(&mut zip, "_rels/.rels", root_rels(), options)?;
    put(
        &mut zip,
        "docProps/core.xml",
        core_properties(title),
        options,
    )?;
    put(
        &mut zip,
        "docProps/app.xml",
        app_properties(images.len()),
        options,
    )?;
    put(
        &mut zip,
        "ppt/presentation.xml",
        presentation(images.len()),
        options,
    )?;
    put(
        &mut zip,
        "ppt/_rels/presentation.xml.rels",
        presentation_rels(images.len()),
        options,
    )?;
    put(
        &mut zip,
        "ppt/slideMasters/slideMaster1.xml",
        slide_master(),
        options,
    )?;
    put(
        &mut zip,
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        slide_master_rels(),
        options,
    )?;
    put(
        &mut zip,
        "ppt/slideLayouts/slideLayout1.xml",
        slide_layout(),
        options,
    )?;
    put(
        &mut zip,
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        slide_layout_rels(),
        options,
    )?;
    put(&mut zip, "ppt/theme/theme1.xml", theme(), options)?;

    for (index, path) in images.iter().enumerate() {
        let number = index + 1;
        put(
            &mut zip,
            &format!("ppt/slides/slide{number}.xml"),
            slide(number),
            options,
        )?;
        put(
            &mut zip,
            &format!("ppt/slides/_rels/slide{number}.xml.rels"),
            slide_rels(number),
            options,
        )?;
        let bytes = std::fs::read(path)
            .map_err(|error| format!("Could not read '{}': {error}", path.display()))?;
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(format!("'{}' is not a PNG image.", path.display()));
        }
        zip.start_file(format!("ppt/media/image{number}.png"), options)
            .map_err(|error| format!("Could not add image {number}: {error}"))?;
        zip.write_all(&bytes)
            .map_err(|error| format!("Could not write image {number}: {error}"))?;
    }

    zip.finish()
        .map_err(|error| format!("Could not finish the PPTX: {error}"))?;
    Ok(())
}

/// Re-open a package and check the parts an independent reader needs.
pub fn verify_pptx(path: &Path, slides: usize) -> Result<(), String> {
    let file = std::fs::File::open(path)
        .map_err(|error| format!("Could not open '{}': {error}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("'{}' is not a valid ZIP: {error}", path.display()))?;
    for index in 1..=slides {
        for name in [
            format!("ppt/slides/slide{index}.xml"),
            format!("ppt/slides/_rels/slide{index}.xml.rels"),
            format!("ppt/media/image{index}.png"),
        ] {
            archive
                .by_name(&name)
                .map_err(|_| format!("The PPTX is missing '{name}'."))?;
        }
        let mut xml = String::new();
        archive
            .by_name(&format!("ppt/slides/slide{index}.xml"))
            .map_err(|_| format!("The PPTX is missing slide {index}."))?
            .read_to_string(&mut xml)
            .map_err(|error| format!("Could not read slide {index}: {error}"))?;
        if !xml.contains(r#"cx="12192000""#) || !xml.contains(r#"cy="6858000""#) {
            return Err(format!(
                "Slide {index} is not a 16:9 full-bleed picture (the 12192000×6858000 transform is missing)."
            ));
        }
        if !xml.contains("r:embed=\"rId2\"") {
            return Err(format!("Slide {index} is not linked to its image."));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn png_1x1() -> Vec<u8> {
        // A tiny valid PNG (1×1, transparent). The PPTX writer only checks the
        // signature; the package verifier checks the structure.
        vec![
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, b'I', b'D', b'A', b'T', 0x78,
            0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00,
            0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82,
        ]
    }

    #[test]
    fn a_minimal_package_has_every_part_and_full_bleed_transforms() {
        let root = std::env::temp_dir().join(format!("rb-pptx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let image = root.join("slide-1.png");
        std::fs::write(&image, png_1x1()).unwrap();
        let deck = root.join("deck.pptx");
        write_pptx(&[&image], &deck, "Test <deck>").unwrap();
        verify_pptx(&deck, 1).unwrap();

        let file = std::fs::File::open(&deck).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut slide = String::new();
        archive
            .by_name("ppt/slides/slide1.xml")
            .unwrap()
            .read_to_string(&mut slide)
            .unwrap();
        assert!(slide.contains(r#"r:embed="rId2""#));
        assert!(slide.contains(r#"cx="12192000""#));
        assert!(slide.contains(r#"cy="6858000""#));
        let mut core = String::new();
        archive
            .by_name("docProps/core.xml")
            .unwrap()
            .read_to_string(&mut core)
            .unwrap();
        assert!(core.contains("Test &lt;deck&gt;"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn two_images_become_two_slides_with_separate_media() {
        let root = std::env::temp_dir().join(format!("rb-pptx-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let one = root.join("one.png");
        let two = root.join("two.png");
        std::fs::write(&one, png_1x1()).unwrap();
        std::fs::write(&two, png_1x1()).unwrap();
        let deck = root.join("deck.pptx");
        write_pptx(&[&one, &two], &deck, "Two").unwrap();
        verify_pptx(&deck, 2).unwrap();
        let _ = std::fs::remove_dir_all(root);
    }
}
