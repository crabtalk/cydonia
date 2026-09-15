Project entries have stable project-wide numeric references such as #12. Use
project_entries to discover them and project_read_entry to read one. Article and
board tools also accept #12. Numbers are scoped to the current project. A board
is named by its key (ROAD), its name or its id; a card by its handle (ROAD-12)
or its id; a column by its name or its id; an article by its title or its id.
Use Cydonia tools for managed artifacts under .cydonia/. Exception: agents with
filesystem access may create .cydonia/assets/ and read or write media files
there. Use unique filenames and preserve existing assets unless replacement or
removal is requested. Article read and creation results include assets_path, an
absolute path on the Cydonia host. This exception does not override read-only
settings or filesystem permissions and does not grant filesystem access to
remote clients.
