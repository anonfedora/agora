'use client'

import group from "../../public/icons/user-group.svg"
import Image from "next/image"

type InfoCard = {
  id: string;
  title: string;
  description: string;
  image : string;
};

const cardsData: InfoCard[] = [
  {
    id: "stellar-west-africa",
    title: "Stellar West Africa",
    description:
      "Building and empowering the Stellar ecosystem in West Africa through education, developer support, and real-world blockchain adoption.",
    image: "/icons/stellar-west-africa.svg"
  },
  {
    id: "stellar-east-african-community",
    title: "Stellar East African Community",
    description:
      "Building and empowering the Stellar ecosystem in East Africa through education, developer support, and real-world blockchain adoption.",
    image: "/icons/stellar-east-africa.svg"
  },
  {
    id: "stellar-india",
    title: "Stellar India",
    description:
      "Building and empowering the Stellar ecosystem in West Africa through education, developer support, and real-world blockchain adoption.",
    image: "/icons/stellar-india.svg"
  },
  {
    id: "stellar-portugal",
    title: "Stellar Portugal",
    description:
      "Building and empowering the Stellar ecosystem in West Africa through education, developer support, and real-world blockchain adoption.",
    image: "/icons/stellar-portugal.svg"
  },
];


const Button : React.FC = () => {
  return (
    <button className="bg-yellow-300 pt-2 pl-3 pr-3 pb-2 flex gap-3 border border-yellow-300 rounded-lg items-center absolute top-40 right-5 hover:cursor-pointer">
      <Image src={group} alt="User Group Icon" className="w-5 h-5 lg:w-6 h-6" />
      <span className="text-black font-semibold">Subscribe</span>
    </button>
  )
}


export function OrganizerComponent() {
  return (
    <div className="p-10 ml-4 lg:ml-40 hidden lg:block">
        <div className="flex justify-start items-center gap-4 p-5 pb-10">
            <h1 className="font-semibold md:text-4xl">Explore organizers</h1>
            <Image src={group} alt="User Group Icon" className="w-5 h-5 lg:w-6 h-6 font-bold" />
        </div>
        <section className="flex justify-center items-center gap-10 overflow-x-auto scrollbar-hide pl-75 mr-50 h-65">
              {cardsData.map((card) => (
                <div key={card.id} className="relative h-full">
                  <section className="absolute border-10 rounded-2xl bg-yellow-400 border-yellow-400 w-102 h-58 -left-2 top-2 z-0"></section>
                  <div className="relative z-10 bg-black text-white p-5x border rounded-2xl lg:min-w-100
                    relative h-40 lg:h-58">
                    <div className="absolute top-5 left-5">
                      <Image src={card.image} alt={card.title} height={65} width={65} className="relative z-10 border-4 border-black rounded-full" />
                      <div className="absolute -left-1 top-1 w-15 h-15 bg-white rounded-full z-0"/>
                    </div>
                    <div className="text-lg font-semibold absolute left-25 top-10 w-full">{card.title}</div>
                    <p className="text-xs absolute left-25 top-20 w-65">{card.description}</p>
                    <Button />
                  </div>
                </div>
              ))}
              
          </section>
    </div>
  )
}