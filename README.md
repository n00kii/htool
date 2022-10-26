# current bugs
 - (FIXED) deleting a link either seems to delete inner media or doesnt set them independant 
 - extracted stuff keeps bytes forever, even after import
 - if something is a duplicate but will belong to a new pool, it should get a link to that new pool (currently doesnt)
 - crash when trying to import everything with extracts (panics with sender dropped) (perhaps data running out of threads?)

# to change
 - make namespaces live in the db, add a field to sharedstate
 - figure out why you cant use threadpool when importing