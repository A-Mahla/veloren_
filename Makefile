
client:
	cargo run

server:
	cargo build --bin veloren-server-cli
	rm -rf server-cli/veloren-server-cli
	mv target/debug/veloren-server-cli server-cli/
	cp -r assets/ server-cli/.
	docker compose -f server-cli/docker-compose.yml up --build 

.PHONY: server client
