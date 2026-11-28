use std::path::Path;

use selflow_platform::service::parse_exec_start;

#[test]
fn the_path_comes_from_exec_start() {
    let output = "{ path=/home/talinux/.cargo/bin/selflowd ; argv[]=/home/talinux/.cargo/bin/selflowd ; ignore_errors=no ; start_time=[n/a] ; stop_time=[n/a] ; pid=0 ; code=(null) ; status=0/0 }";
    assert_eq!(
        parse_exec_start(output).as_deref(),
        Some(Path::new("/home/talinux/.cargo/bin/selflowd"))
    );
}

#[test]
fn no_exec_start_means_no_path() {
    for output in ["", "\n", "{ argv[]=/x ; }"] {
        assert_eq!(parse_exec_start(output), None, "{output:?}");
    }
}

#[test]
fn a_path_that_is_not_absolute_gives_no_path() {
    for output in [
        "{ path=selflowd ; argv[]=selflowd ; }",
        "{ path=.cargo/bin/selflowd ; argv[]=.cargo/bin/selflowd ; }",
        "{ path= ; argv[]=/x ; }",
    ] {
        assert_eq!(parse_exec_start(output), None, "{output:?}");
    }
}
