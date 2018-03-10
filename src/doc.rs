extern crate failure;
extern crate handlebars;
extern crate walkdir;

use std::fs::{create_dir_all, File};
use std::io::{self, Read};
use std::path::Path;

use self::failure::ResultExt;

use base::filename_to_module;
use base::types::ArcType;
use base::metadata::Metadata;
use check::metadata::metadata;
use {Compiler, Thread};

pub type Error = failure::Error;
pub type Result<T> = ::std::result::Result<T, Error>;

#[derive(Serialize, PartialEq, Debug)]
pub struct Module<'a> {
    pub name: &'a str,
    pub record: Record<'a>,
}

#[derive(Serialize, PartialEq, Debug)]
pub struct Record<'a> {
    pub types: Vec<Field<'a>>,
    pub values: Vec<Field<'a>>,
}

#[derive(Serialize, PartialEq, Debug)]
pub struct Field<'a> {
    pub name: &'a str,
    #[serde(rename = "type")]
    pub typ: String,
    pub comment: &'a str,
}

pub fn record<'a>(typ: &'a ArcType, meta: &'a Metadata) -> Record<'a> {
    Record {
        types: typ.type_field_iter()
            .map(|field| Field {
                name: field.name.as_ref(),
                typ: field.typ.unresolved_type().to_string(),
                comment: meta.module
                    .get(AsRef::<str>::as_ref(&field.name))
                    .and_then(|meta| meta.comment.as_ref().map(|s| &s[..]))
                    .unwrap_or(""),
            })
            .collect(),

        values: typ.row_iter()
            .map(|field| Field {
                name: field.name.as_ref(),
                typ: field.typ.to_string(),

                comment: meta.module
                    .get(AsRef::<str>::as_ref(&field.name))
                    .and_then(|meta| meta.comment.as_ref().map(|s| &s[..]))
                    .unwrap_or(""),
            })
            .collect(),
    }
}

pub fn generate<W>(out: &mut W, name: &str, typ: &ArcType, meta: &Metadata) -> Result<()>
where
    W: io::Write,
{
    let r = Module {
        name,
        record: record(typ, meta),
    };

    trace!("DOC: {:?}", r);

    let reg = handlebars::Handlebars::new();
    let module_template = include_str!("doc/module.html");
    reg.render_template_to_write(module_template, &r, out)?;
    Ok(())
}

pub fn generate_for_path<P, Q>(thread: &Thread, path: &P, out_path: &Q) -> Result<()>
where
    P: ?Sized + AsRef<Path>,
    Q: ?Sized + AsRef<Path>,
{
    for entry in walkdir::WalkDir::new(path) {
        let entry = entry?;
        if !entry.file_type().is_file()
            || entry.path().extension().and_then(|ext| ext.to_str()) != Some("glu")
        {
            continue;
        }
        let mut input = File::open(&*entry.path()).with_context(|err| {
            format!(
                "Unable to open gluon file `{}`: {}",
                entry.path().display(),
                err
            )
        })?;
        let mut content = String::new();
        input.read_to_string(&mut content)?;

        let (expr, typ) = Compiler::new().typecheck_str(thread, "basic", &content, None)?;
        let (meta, _) = metadata(&*thread.get_env(), &expr);

        create_dir_all(
            out_path
                .as_ref()
                .join(entry.path().parent().unwrap_or(Path::new(""))),
        )?;

        let out_path = out_path.as_ref().join(entry.path().with_extension("html"));
        let mut doc_file = File::create(&*out_path).with_context(|err| {
            format!(
                "Unable to open output file `{}`: {}",
                out_path.display(),
                err
            )
        })?;

        let name = filename_to_module(entry
            .path()
            .to_str()
            .ok_or_else(|| failure::err_msg("Non-UTF-8 filename"))?);
        generate(&mut doc_file, &name, &typ, &meta)?;
    }
    Ok(())
}
