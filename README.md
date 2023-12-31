# Web

## How to

### Dependencies

The solution was tested on NixOS and the Ubuntu docker. To run the solution on Ubuntu:

`apt update && apt install -y rustc cargo pkg-config libssl-dev`

### Build

`cargo build`

### Test

`cargo test`

### Run

`cargo run -p web`

### Request

`curl http://127.0.0.1:5000/report`
`curl -X POST http://127.0.0.1:5000/transactions -F "data=@data.csv"`

## Approach & Assumptions

General: Web is a web server that is backed by SQLite (with WAL) to manage the reports. Each transaction is added to a transaction table and for each CSV a new report entry is added to the report table. To obtain the report, the server obtains a list of all the reports and performs a sum over all of them.

Accuracy: since the terms are financial numbers, they need to be exact. As such, all of the arithmetic is done via the Decimal library, inside the code, as opposed to doing a sum via SQL.

CSV: the CSV is expected to have a date in the Y-M-D format. The web server will perform in a best effort manner, it will try to add as many valid csv entries in the CSV file, atomically to the database at once. For example, if 5 entries in a CSV file are valid, either all of them will be committed together or none of them will.

Concurrency: the database can handle concurrent writes and reads Due to limitations of SQLite, some operations may be denied due to congestion (i.e. if multiple writes and multiple reads happen at the same time). Currently, a pool of 50 connections spawn during startup. The code was tested with parallelized and sequential requests. In the parallel case, depending on the size of the CSV, some requests may be rejected due to congestion. This performance is acceptable as the application requirements are much less rigorous.

CSV Size: roughly, a maximum of 5000 records can be sent in each CSV, as either the request will be denied by the web server due to size (2MiB), or the number of terms in a single SQL request will overflow.

## Shortcomings

CSV parsing in general can further be improved to accept more types or to be more/less strict depending on the policy.

## Further improvements

A few to note: testing, error messages, comments, concurrency and scalability can further be improved.
