# current bugs
 - (FIXED) deleting a link either seems to delete inner media or doesnt set them independant 
 - extracted stuff keeps bytes forever, even after import
 - if something is a duplicate but will belong to a new pool, it should get a link to that new pool (currently doesnt)
 - crash when trying to import everything with extracts (panics with sender dropped) (perhaps data running out of threads?)
 - deleting (or adding) a media of a pool needs to update any living entry_info of that pool

# to change
 - make namespaces live in the db, add a field to sharedstate
 - figure out why you cant use threadpool when importing
 - implement logging
 - implement view of links, way to remove from lin
 - implement selection options in galler