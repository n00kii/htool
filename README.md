# current bugs
 - (FIXED) deleting a link either seems to delete inner media or doesnt set them independant 
 - (FIXED) extracted stuff keeps bytes forever, even after import
 - if something is a duplicate but will belong to a new pool, it should get a link to that new pool (currently doesnt)
 - crash when trying to import everything with extracts (panics with sender dropped) (perhaps data running out of threads?)
 - (FIXED) deleting (or adding) a media of a pool needs to update any living entry_info of that pool
 - (FIXED) removing media from link updates globally updates pool previews, not but media previews
 - tags ui should be using threads for tag ops

# to change
 - make namespaces live in the db, add a field to sharedstate
 - figure out why you cant use threadpool when importing
 - implement logging (tracing)
 - implement profiling (puffin)
 - implement view of links, way to remove from lin
 - implement selection options in galler
 - use "with requests" loading for pools
 - figure out how to make preview windows use strips (weird tag list sizing)
 - exts filter for importing should be whitelist, not blacklist
 - make importer use shared state
 - encryption
 