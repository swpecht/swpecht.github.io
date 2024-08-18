package main

import (
	"bufio"
	"fmt"
	"log"
	"net/http"
	"os"
	"os/exec"

	"github.com/creack/pty"
	"gopkg.in/yaml.v3"
)

type Secrets struct {
	Token string
}

func main() {

	http.HandleFunc("/events", eventsHandler)
	http.Handle("/", http.FileServer(http.Dir("./dist")))
	http.ListenAndServe(":3000", nil)
	// 	_, err := gorm.Open(sqlite.Open("test.db"), &gorm.Config{})
	// 	if err != nil {
	// 		panic("failed to connect database")
	// 	}
	// 	secrets := getSecrets()
	// 	client := godo.NewFromToken(secrets.Token)
	// 	list, _, err := client.Droplets.List(context.TODO(), &godo.ListOptions{})

	// 	for _, drop := range list {
	// 		fmt.Println(drop.Name, drop.Tags, drop.ID)
	// 	}

	// _, _, err = client.Droplets.Create(context.TODO(), &godo.DropletCreateRequest{
	// 	Name:   "kiwi-worker-test",
	// 	Region: "nyc3",
	// 	Size:   "s-2vcpu-4gb",
	// 	Image: godo.DropletCreateImage{
	// 		ID:   0,
	// 		Slug: "ubuntu-20-04-x64",
	// 	},
	// 	SSHKeys:           []godo.DropletCreateSSHKey{},
	// 	Backups:           false,
	// 	IPv6:              true,
	// 	PrivateNetworking: false,
	// 	Monitoring:        false,
	// 	UserData:          "",
	// 	Volumes:           []godo.DropletCreateVolume{},
	// 	Tags:              []string{"env:test", "kiwi", "worker"},
	// 	VPCUUID:           "",
	// 	WithDropletAgent:  new(bool),
	// })
	// fmt.Printf("%v, %v, %v", list, resp, err)

	// client.Droplets.Delete(context.TODO(), 407293232)
}

func eventsHandler(w http.ResponseWriter, r *http.Request) {
	println("accepted events connection")

	// Set CORS headers to allow all origins. You may want to restrict this to specific origins in a production environment.
	w.Header().Set("Access-Control-Allow-Origin", "*")
	w.Header().Set("Access-Control-Expose-Headers", "Content-Type")

	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")

	// cmd := exec.Command("top")

	// stderr, _ := cmd.StderrPipe()
	// cmd.Start()

	// scanner := bufio.NewScanner(stderr)
	// scanner.Split(bufio.ScanWords)
	// for scanner.Scan() {
	// 	m := scanner.Text()
	// 	println(m)
	// 	fmt.Fprintf(w, "data: %s\n\n", m)
	// 	w.(http.Flusher).Flush()
	// 	time.Sleep(1 * time.Second)
	// }

	// might just want to do std out to start, ping doesn't work with tty
	c := exec.Command("top")

	f, err := pty.Start(c)
	// pty.Setsize(f, &pty.Winsize{})
	if err != nil {
		panic(err)
	}

	// get inheret size working: https://github.com/creack/pty

	scanner := bufio.NewScanner(f)
	scanner.Split(bufio.ScanRunes)
	for scanner.Scan() {
		m := scanner.Text()
		fmt.Fprintf(w, "data: %s\n\n", m)
		w.(http.Flusher).Flush()
	}

	// io.Copy(w, f)

	// cmd.Wait()

	// Simulate closing the connection
	closeNotify := w.(http.CloseNotifier).CloseNotify()
	<-closeNotify
}

func getSecrets() Secrets {

	secrets := Secrets{}
	yamlFile, err := os.ReadFile("secrets.yaml")
	if err != nil {
		log.Printf("yamlFile.Get err   #%v ", err)
	}
	err = yaml.Unmarshal(yamlFile, &secrets)
	if err != nil {
		log.Fatalf("Unmarshal: %v", err)
	}

	return secrets
}
