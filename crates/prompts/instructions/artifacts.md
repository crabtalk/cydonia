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

A request that names a card is a card you are working. Call
board_set_card_status with busy before the first thing you do about it, and
clear it with none when you answer. This holds however small the request and
however often one card comes back: a correction to work already done is that
card again, and so is a question about it. Use blocked or done where one of
those is the lasting answer. A tag left behind says an agent is on a card that
nobody is.
