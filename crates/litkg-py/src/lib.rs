use litkg_core::{
    build_tabular_bundle_from_parsed, build_tabular_bundle_from_parsed_with_notebooks,
    load_notebook_documents, load_parsed_papers, write_tabular_exports, ParsedPaper,
};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

fn to_py_err(error: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

fn load_parsed(parsed_root: &str) -> PyResult<Vec<ParsedPaper>> {
    load_parsed_papers(parsed_root).map_err(to_py_err)
}

#[pyfunction]
fn load_parsed_papers_json(parsed_root: &str) -> PyResult<String> {
    let papers = load_parsed(parsed_root)?;
    serde_json::to_string(&papers).map_err(to_py_err)
}

#[pyfunction]
fn build_tabular_bundle_json(parsed_root: &str) -> PyResult<String> {
    let papers = load_parsed(parsed_root)?;
    let bundle = build_tabular_bundle_from_parsed(&papers);
    serde_json::to_string(&bundle).map_err(to_py_err)
}

#[pyfunction]
fn build_tabular_bundle_with_notebooks_json(
    parsed_root: &str,
    notebook_root: &str,
) -> PyResult<String> {
    let papers = load_parsed(parsed_root)?;
    let bundle = build_tabular_bundle_from_parsed_with_notebooks(&papers, notebook_root)
        .map_err(to_py_err)?;
    serde_json::to_string(&bundle).map_err(to_py_err)
}

#[pyfunction]
fn load_notebooks_json(notebook_root: &str) -> PyResult<String> {
    let notebooks = load_notebook_documents(notebook_root).map_err(to_py_err)?;
    serde_json::to_string(&notebooks).map_err(to_py_err)
}

#[pyfunction]
fn write_tabular_exports_from_parsed(parsed_root: &str, output_root: &str) -> PyResult<()> {
    let papers = load_parsed(parsed_root)?;
    let bundle = build_tabular_bundle_from_parsed(&papers);
    write_tabular_exports(output_root, &bundle).map_err(to_py_err)?;
    Ok(())
}

#[pyfunction]
fn write_tabular_exports_from_parsed_with_notebooks(
    parsed_root: &str,
    notebook_root: &str,
    output_root: &str,
) -> PyResult<()> {
    let papers = load_parsed(parsed_root)?;
    let bundle = build_tabular_bundle_from_parsed_with_notebooks(&papers, notebook_root)
        .map_err(to_py_err)?;
    write_tabular_exports(output_root, &bundle).map_err(to_py_err)?;
    Ok(())
}

#[pymodule]
fn _native(_py: Python<'_>, module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(load_parsed_papers_json, module)?)?;
    module.add_function(wrap_pyfunction!(load_notebooks_json, module)?)?;
    module.add_function(wrap_pyfunction!(build_tabular_bundle_json, module)?)?;
    module.add_function(wrap_pyfunction!(
        build_tabular_bundle_with_notebooks_json,
        module
    )?)?;
    module.add_function(wrap_pyfunction!(write_tabular_exports_from_parsed, module)?)?;
    module.add_function(wrap_pyfunction!(
        write_tabular_exports_from_parsed_with_notebooks,
        module
    )?)?;
    Ok(())
}
