error_chain! {
    foreign_links {
        Io(::std::io::Error);
    }

    errors {
        PingInternalError
    }
}
